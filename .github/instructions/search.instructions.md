---
applyTo: "crates/miniplan/src/search.rs,crates/miniplan/src/search/**,crates/miniplan/src/heuristic.rs,crates/miniplan/src/heuristic/**,crates/miniplan/tests/it/plan_validity.rs,crates/miniplan/tests/it/pop.rs,crates/miniplan/benches/**"
---

<!-- generated from docs/review/search.md by scripts/review-index.py; edit the source instead -->

# Planners and heuristics

- Every planner terminates on finite tasks without limits. It returns `Unsolvable` once its
  search space is exhausted. Bidirectional planners must not rely on the backward
  (regression) frontier running dry, because it can generate subgoals forever.
  Exception: the plan-space planner `pop` searches an infinite space. It proves
  unsolvability only when the relaxation shows the goal unreachable, and otherwise runs
  into a limit. Its tests (`tests/it/pop.rs`) give it a node budget on unsolvable cases.
- Every returned plan is executable: each step's operator is applicable in sequence from
  `task.init`, and the final state satisfies the goal. `plan_validity.rs` checks this for every
  planner it lists. A new planner is added there (`pop` lives in `tests/it/pop.rs`, which
  reuses the same generators and oracles, for the termination exception above).
- Optimality claims in docs (`describe()`, README, module docs) must hold. A planner marked
  optimal in `plan_validity.rs` must return shortest plans. Optimal planners that take a
  heuristic need an admissible one (`hmax`, `zero`, `blind`). `hff` and `hadd` are not
  admissible.
- `SearchLimits` (time and node budgets) are checked in the main loop of every planner, and
  hitting one returns `LimitReached`, never a partial plan.
- `SearchOutcome` is `#[non_exhaustive]`. New variants are a minor-version change, and
  examples matching on it keep a wildcard arm.
- Planners that do not support a task feature (conditional effects, derived predicates)
  return an error such as `UnsupportedConditionalEffects` instead of searching wrongly.
  `PlannerCapabilities` matches what the planner really supports.
- `SearchStats` fields (`plan_cost`, `plan_length`, `nodes_expanded`) agree with the
  returned plan.
- Performance changes to a hot path come with `task bench` numbers in the PR (before and
  after).
- Grounding semantics belong to the task model, not to individual planners. A planner must
  not reinterpret operators.
