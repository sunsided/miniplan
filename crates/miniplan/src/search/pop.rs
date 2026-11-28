//! Partial-order causal-link planning ([`Pop`]): search in plan space instead of state space.
//!
//! A search node is a partial plan (see [`partial_plan`]). Each expansion repairs one flaw
//! of it, branching over the ways to repair that flaw:
//!
//! ```mermaid
//! flowchart TD
//!     pop[pop cheapest partial plan] --> threat{threatened link?}
//!     threat -- yes --> order[branch: demote or promote the threatening step]
//!     threat -- no --> open{open condition?}
//!     open -- no --> done[linearize: plan found]
//!     open -- yes --> pick[pick the one with fewest resolvers]
//!     pick --> resolve[branch: link an existing step or add a new one]
//!     order --> push[rank children, push]
//!     resolve --> push
//! ```
//!
//! The planner is grounded: steps are operators of the [`Task`], so there are no variable
//! bindings to commit to. It is sound and complete for STRIPS with negative preconditions,
//! but not optimal: every step ranks at a cost of at least 1, so only finitely many partial
//! plans rank below any bound. Plan space is infinite, so unless the relaxation proves the goal
//! unreachable an unsolvable task usually runs into a [`SearchLimits`] limit instead of
//! ending as [`SearchOutcome::Unsolvable`].

mod partial_plan;

use std::collections::BinaryHeap;

use crate::error::MiniplanError;
use crate::heuristic::{rpg_add_fact_costs, rpg_fact_costs};
use crate::search::open_list::OpenEntry;
use crate::search::outcome::{Budget, reject_conditional_effects};
use crate::search::{Planner, PlannerCapabilities, SearchLimits, SearchOutcome, SearchStats, Task};

use partial_plan::{GOAL, INIT, Link, Literal, OpenCondition, PartialPlan, Threat};

/// How [`Pop`] estimates the remaining cost of a partial plan from its open conditions.
///
/// The priority of a partial plan is the cost of its steps (each at least 1) plus the summed
/// estimates of its open conditions, so every variant also counts the steps so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopRanking {
    /// Each positive open condition costs its h^add value from the initial state (VHPOP's
    /// `ADD` ranking). The default; also selected by `hff`, whose relaxed exploration it is.
    #[default]
    HAdd,
    /// Each positive open condition costs its h^max value from the initial state.
    HMax,
    /// Every open condition costs 1, as in SNLP and UCPOP.
    GoalCount,
    /// Open conditions cost nothing: uniform-cost search over the plan cost. Also
    /// selected by `blind`.
    Zero,
}

impl PopRanking {
    /// The ranking selected by the `heuristic` option `name`.
    ///
    /// Every built-in heuristic name is accepted, so the default `hff` of the CLI works.
    ///
    /// # Errors
    ///
    /// Returns [`MiniplanError::InvalidHeuristic`] for any name other than `hadd`, `hff`,
    /// `hmax`, `goal-count`, `blind` and `zero`.
    ///
    /// # Examples
    ///
    /// ```
    /// use miniplan::search::PopRanking;
    ///
    /// assert_eq!(PopRanking::from_name("hmax").unwrap(), PopRanking::HMax);
    /// assert!(PopRanking::from_name("lm-cut").is_err());
    /// ```
    pub fn from_name(name: &str) -> Result<Self, MiniplanError> {
        match name {
            "hadd" | "hff" => Ok(Self::HAdd),
            "hmax" => Ok(Self::HMax),
            "goal-count" => Ok(Self::GoalCount),
            "blind" | "zero" => Ok(Self::Zero),
            _ => Err(MiniplanError::InvalidHeuristic(name.to_owned())),
        }
    }
}

/// Partial-order causal-link planner (grounded SNLP/UCPOP with VHPOP-style ranking).
///
/// Returns plans whose [`Plan::partial_order`](crate::plan::Plan::partial_order) holds the
/// ordering constraints the plan actually needs. Rejects conditional effects.
///
/// # Examples
///
/// ```
/// use miniplan::search::{Planner, Pop, PopRanking};
///
/// let planner = Pop::new(PopRanking::HAdd);
/// assert_eq!(planner.name(), "pop");
/// ```
#[derive(Debug, Default)]
pub struct Pop {
    /// How partial plans are ranked.
    ranking: PopRanking,
}

impl Pop {
    /// Create a partial-order planner that ranks partial plans by `ranking`.
    #[must_use]
    pub fn new(ranking: PopRanking) -> Self {
        Self { ranking }
    }
}

impl Planner for Pop {
    fn name(&self) -> &'static str {
        "pop"
    }

    fn describe(&self) -> &'static str {
        "Partial-order causal-link planner"
    }

    fn capabilities(&self) -> PlannerCapabilities {
        PlannerCapabilities::CLASSICAL
            | PlannerCapabilities::NEGATIVE_PRECONDS
            | PlannerCapabilities::ACTION_COSTS
    }

    /// Runs best-first search over partial plans.
    ///
    /// # Errors
    ///
    /// Returns [`MiniplanError::UnsupportedConditionalEffects`] if any operator has a
    /// conditional effect.
    fn solve(
        &mut self,
        task: &Task,
        limits: &SearchLimits,
    ) -> Result<SearchOutcome, MiniplanError> {
        reject_conditional_effects(task)?;

        let budget = Budget::start(limits);
        let search = Search {
            task,
            ranker: Ranker::new(task, self.ranking),
            achievers: achievers(task),
        };

        Ok(search.run(&budget))
    }
}

/// Estimates partial-plan priorities; see [`PopRanking`].
struct Ranker {
    /// The selected ranking.
    ranking: PopRanking,
    /// Relaxed cost of each fact from the initial state, by fact index: h^add for
    /// [`PopRanking::HAdd`], h^max otherwise. Infinite for facts no plan can reach.
    fact_cost: Vec<f64>,
}

impl Ranker {
    /// Runs the relaxed exploration from the initial state of `task` once.
    fn new(task: &Task, ranking: PopRanking) -> Self {
        let fact_cost = match ranking {
            PopRanking::HAdd => rpg_add_fact_costs(task, &task.init),
            _ => rpg_fact_costs(task, &task.init),
        };

        Self { ranking, fact_cost }
    }

    /// The priority of `plan`: its ranking step cost plus the estimates of its open
    /// conditions.
    ///
    /// Infinite when some open condition is unreachable even in the relaxation, which
    /// ignores deletes and negative preconditions, so such a plan has no completion.
    fn priority(&self, task: &Task, plan: &PartialPlan) -> f64 {
        plan.rank_cost
            + plan
                .open
                .iter()
                .map(|open| self.literal_cost(task, open.literal))
                .sum::<f64>()
    }

    /// The estimated cost of achieving `literal` from the initial state.
    ///
    /// shortcut: a negative literal costs 0 if false initially and 1 otherwise; upgrade to
    /// the cheapest deleter if domains with many negative conditions rank badly.
    fn literal_cost(&self, task: &Task, literal: Literal) -> f64 {
        let relaxed = self.fact_cost[literal.fact];
        if literal.positive && relaxed.is_infinite() {
            return f64::INFINITY;
        }

        match self.ranking {
            PopRanking::Zero => 0.0,
            PopRanking::GoalCount => 1.0,
            PopRanking::HAdd | PopRanking::HMax if literal.positive => relaxed,
            PopRanking::HAdd | PopRanking::HMax => {
                if task.init.0.contains(literal.fact) {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

/// Operators achieving each literal, indexed by [`literal_index`].
type Achievers = Vec<Vec<usize>>;

/// The index of `literal` into [`Achievers`].
fn literal_index(literal: Literal) -> usize {
    literal.fact * 2 + usize::from(literal.positive)
}

/// For every literal, the operators that achieve it, in operator order.
///
/// Matches [`partial_plan::operator_achieves`]: deletes win over adds.
fn achievers(task: &Task) -> Achievers {
    let mut index = vec![Vec::new(); task.num_facts() * 2];
    for (op_idx, op) in task.operators.iter().enumerate() {
        let added = op.add.0.ones().filter(|&fact| !op.del.0.contains(fact));
        for fact in added {
            index[literal_index(Literal {
                fact,
                positive: true,
            })]
            .push(op_idx);
        }

        for fact in op.del.0.ones() {
            index[literal_index(Literal {
                fact,
                positive: false,
            })]
            .push(op_idx);
        }
    }

    index
}

/// One run of the plan-space search on [`Search::task`].
struct Search<'t> {
    /// The task being solved.
    task: &'t Task,
    /// Ranks the partial plans.
    ranker: Ranker,
    /// The achievers of every literal.
    achievers: Achievers,
}

impl Search<'_> {
    /// Best-first search from the null plan until a plan without flaws is popped, the
    /// frontier runs empty, or `budget` is used up.
    fn run(&self, budget: &Budget<'_>) -> SearchOutcome {
        let mut stats = SearchStats::default();
        let mut frontier = BinaryHeap::new();
        self.push(&mut frontier, PartialPlan::null(self.task));

        while let Some(entry) = frontier.pop() {
            if budget.exhausted(&stats) {
                return budget.limit_reached(stats);
            }

            stats.nodes_expanded += 1;
            let Some(children) = self.refine(&entry.node) else {
                return budget.plan(entry.node.to_plan(self.task), stats);
            };

            for child in children {
                stats.nodes_generated += 1;
                self.push(&mut frontier, child);
            }
        }

        budget.unsolvable(stats)
    }

    /// Pushes `plan` with its priority, dropping it if it has no completion.
    fn push(&self, frontier: &mut BinaryHeap<OpenEntry<PartialPlan>>, plan: PartialPlan) {
        let priority = self.ranker.priority(self.task, &plan);
        if priority.is_finite() {
            frontier.push(OpenEntry {
                priority,
                g: plan.rank_cost,
                node: plan,
                epoch: 0,
            });
        }
    }

    /// The refinements of `plan` that repair one flaw, or `None` if `plan` has no flaw.
    ///
    /// Threats are repaired first; otherwise the open condition with the fewest resolvers.
    fn refine(&self, plan: &PartialPlan) -> Option<Vec<PartialPlan>> {
        if let Some(threat) = plan.first_threat(self.task) {
            return Some(resolve_threat(plan, threat));
        }

        let index = self.most_constrained_open_condition(plan)?;

        Some(self.resolve_open_condition(plan, index))
    }

    /// The index into `plan.open` of the open condition with the fewest resolvers, the
    /// latest one on ties; `None` if nothing is open.
    fn most_constrained_open_condition(&self, plan: &PartialPlan) -> Option<usize> {
        plan.open
            .iter()
            .enumerate()
            .min_by_key(|&(index, open)| (self.num_resolvers(plan, *open), usize::MAX - index))
            .map(|(index, _)| index)
    }

    /// How many refinements [`Self::resolve_open_condition`] would try for `open`.
    fn num_resolvers(&self, plan: &PartialPlan, open: OpenCondition) -> usize {
        let reusable = (0..plan.num_steps())
            .filter(|&step| can_support(self.task, plan, step, open))
            .count();

        reusable + self.achievers[literal_index(open.literal)].len()
    }

    /// The refinements that support `plan.open[index]`: a link from each existing step that
    /// can supply it, then a link from a new step for each achieving operator.
    fn resolve_open_condition(&self, plan: &PartialPlan, index: usize) -> Vec<PartialPlan> {
        let mut base = plan.clone();
        let open = base.open.remove(index);
        let mut children = Vec::new();

        for step in 0..base.num_steps() {
            if can_support(self.task, &base, step, open) {
                let mut child = base.clone();
                if child.add_link(link(step, open)) {
                    children.push(child);
                }
            }
        }

        for &op in &self.achievers[literal_index(open.literal)] {
            let mut child = base.clone();
            let step = child.add_step(self.task, op);
            if child.add_link(link(step, open)) {
                children.push(child);
            }
        }

        children
    }
}

/// Whether existing `step` of `plan` can supply `open`: it achieves the literal and the
/// order still allows it before the consumer.
fn can_support(task: &Task, plan: &PartialPlan, step: usize, open: OpenCondition) -> bool {
    step != open.consumer
        && !plan.is_before(open.consumer, step)
        && plan.achieves(task, step, open.literal)
}

/// The causal link from `producer` to the consumer of `open`.
fn link(producer: usize, open: OpenCondition) -> Link {
    Link {
        producer,
        literal: open.literal,
        consumer: open.consumer,
    }
}

/// The refinements that remove `threat` from `plan`: demotion (the threatening step before
/// the producer) and promotion (after the consumer), where the sentinels allow it.
fn resolve_threat(plan: &PartialPlan, threat: Threat) -> Vec<PartialPlan> {
    let demotion = (threat.link.producer != INIT).then_some((threat.step, threat.link.producer));
    let promotion = (threat.link.consumer != GOAL).then_some((threat.link.consumer, threat.step));

    demotion
        .into_iter()
        .chain(promotion)
        .filter_map(|(before, after)| {
            let mut child = plan.clone();
            child.order(before, after).then_some(child)
        })
        .collect()
}

#[cfg(test)]
mod tests;
