//! Open lists for the cost-aware bidirectional planners.
//!
//! [`OpenEntry`] turns [`BinaryHeap`] into a min-heap over one `f64` priority. [`Frontier`]
//! keeps one search direction of [`Nbs`](super::Nbs) or [`Bae`](super::Bae): several heaps
//! over the same nodes, each ordered by a different priority, with lazy deletion of
//! outdated entries through per-node epochs.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::hash::Hash;

use rustc_hash::FxHashMap;

use crate::search::outcome::{ParentMap, ROOT_OP};
use crate::task::OpId;

/// One entry of a min-heap open list.
///
/// Ordering and equality look at [`OpenEntry::priority`] only, reversed so that
/// [`BinaryHeap::pop`] returns the smallest priority. Incomparable priorities (NaN) count as
/// equal.
#[derive(Clone)]
pub(super) struct OpenEntry<T> {
    /// The value the heap is ordered by, such as `g`, `f = g + h` or BAE*'s `b`.
    pub(super) priority: f64,
    /// Path cost from the search root to [`OpenEntry::node`] when this entry was pushed.
    pub(super) g: f64,
    /// The search node: a state forward, a subgoal backward.
    pub(super) node: T,
    /// Epoch of the push that created this entry; see [`Frontier`]. Always `0` where
    /// epochs are not used.
    pub(super) epoch: u64,
}

impl<T> PartialEq for OpenEntry<T> {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

impl<T> Eq for OpenEntry<T> {}

impl<T> PartialOrd for OpenEntry<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for OpenEntry<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .priority
            .partial_cmp(&self.priority)
            .unwrap_or(Ordering::Equal)
    }
}

/// The epoch of an expanded node. No entry carries it, so a closed node has no live entry.
const CLOSED: u64 = u64::MAX;

/// One direction of a multi-queue bidirectional search with `N` heaps over the same nodes.
///
/// Every push of a node goes to all `N` heaps with a fresh epoch. An entry is live while its
/// `g` and epoch still match the node's current ones; expanding a node gives it the
/// [`CLOSED`] epoch, so all its entries die until it is reopened at a lower `g`. Dead entries
/// are dropped lazily, whenever they reach the top of a heap. Heap `0` picks the node to
/// expand. The last heap must be ordered by `g`, so its minimum priority is the smallest open
/// `g`.
pub(super) struct Frontier<T, const N: usize> {
    // Every heap holds a live entry for exactly the open nodes, so the heap minima are the
    // smallest open priorities that the planners' stopping bounds rely on.
    /// The open lists, one per priority.
    heaps: [BinaryHeap<OpenEntry<T>>; N],
    /// Best known path cost per node.
    g: FxHashMap<T, f64>,
    /// Epoch of the latest push per node. The root has epoch `0`.
    epochs: FxHashMap<T, u64>,
    /// Parent links of the search tree, for plan reconstruction.
    parents: ParentMap<T>,
    /// The epoch handed to the most recent push. Starts at `0`, which is the root's epoch.
    last_epoch: u64,
}

impl<T: Clone + Eq + Hash, const N: usize> Frontier<T, N> {
    /// Creates a frontier with empty heaps and no nodes.
    pub(super) fn new() -> Self {
        Self {
            heaps: std::array::from_fn(|_| BinaryHeap::new()),
            g: FxHashMap::default(),
            epochs: FxHashMap::default(),
            parents: ParentMap::default(),
            last_epoch: 0,
        }
    }

    /// Opens the search root with `g = 0`, epoch `0` and one priority per heap.
    pub(super) fn push_root(&mut self, root: T, priorities: [f64; N]) {
        self.push_entries(&root, 0.0, priorities, 0);
        self.g.insert(root.clone(), 0.0);
        self.parents.insert(root.clone(), (None, ROOT_OP));
        self.epochs.insert(root, 0);
    }

    /// Returns `true` if reaching `node` at cost `g` beats its best known cost.
    pub(super) fn improves(&self, node: &T, g: f64) -> bool {
        g < self.g_of(node)
    }

    /// Opens `node` at cost `g`, reached from `parent` through `op`, under a fresh epoch.
    ///
    /// Earlier entries of `node` become outdated.
    pub(super) fn push(&mut self, node: T, g: f64, priorities: [f64; N], parent: T, op: OpId) {
        self.last_epoch += 1;
        let epoch = self.last_epoch;

        self.push_entries(&node, g, priorities, epoch);
        self.g.insert(node.clone(), g);
        self.parents.insert(node.clone(), (Some(parent), op));
        self.epochs.insert(node, epoch);
    }

    /// The smallest priority of an open node in heap `heap`, or `None` if no node is open.
    pub(super) fn min_priority(&mut self, heap: usize) -> Option<f64> {
        while let Some(top) = self.heaps[heap].peek() {
            if self.is_live(top) {
                return Some(top.priority);
            }

            self.heaps[heap].pop();
        }

        None
    }

    /// Returns `true` once no node is open.
    pub(super) fn is_exhausted(&mut self) -> bool {
        self.min_priority(0).is_none()
    }

    /// Pops the open node with the smallest heap-`0` priority for expansion, with its `g`,
    /// and closes it. Returns `None` if no node is open.
    pub(super) fn pop_expansion(&mut self) -> Option<(T, f64)> {
        let chosen = self.pop_live(0)?;
        self.epochs.insert(chosen.node.clone(), CLOSED);

        Some((chosen.node, chosen.g))
    }

    /// Parent links of the nodes opened so far.
    pub(super) fn parents(&self) -> &ParentMap<T> {
        &self.parents
    }

    /// Pushes one entry for `node` into every heap.
    fn push_entries(&mut self, node: &T, g: f64, priorities: [f64; N], epoch: u64) {
        for (heap, priority) in self.heaps.iter_mut().zip(priorities) {
            heap.push(OpenEntry {
                priority,
                g,
                node: node.clone(),
                epoch,
            });
        }
    }

    /// Pops entries from heap `heap` until a live one turns up, or the heap is empty.
    fn pop_live(&mut self, heap: usize) -> Option<OpenEntry<T>> {
        while let Some(entry) = self.heaps[heap].pop() {
            if self.is_live(&entry) {
                return Some(entry);
            }
        }

        None
    }

    /// Returns `true` if `entry` still describes its node's latest push.
    #[expect(
        clippy::float_cmp,
        reason = "`g` is copied, never recomputed, so a live entry matches bit for bit"
    )]
    fn is_live(&self, entry: &OpenEntry<T>) -> bool {
        let epoch = self.epochs.get(&entry.node).copied().unwrap_or(0);

        entry.g == self.g_of(&entry.node) && entry.epoch == epoch
    }

    /// Best known path cost of `node`, or infinity if it was never opened.
    fn g_of(&self, node: &T) -> f64 {
        self.g.get(node).copied().unwrap_or(f64::INFINITY)
    }
}

/// Decides which direction expands next from the smallest open priorities of both sides.
///
/// Forward wins ties. A side without open entries never expands; with both empty this
/// returns `false`.
pub(super) fn prefer_forward(forward_min: Option<f64>, backward_min: Option<f64>) -> bool {
    match (forward_min, backward_min) {
        (None, _) => false,
        (_, None) => true,
        (Some(forward), Some(backward)) => forward <= backward,
    }
}

/// Lower bound on the cost of any plan not found yet, from the smallest open `f` and `g`
/// values of both directions (the bound NBS and BAE* stop on).
///
/// With both sides open it is `max(f_fwd, f_bwd, g_fwd + g_bwd)`. With one side empty only
/// that side's minima count, and with neither it is infinite.
pub(super) fn frontier_bound(
    f_forward: Option<f64>,
    f_backward: Option<f64>,
    g_forward: Option<f64>,
    g_backward: Option<f64>,
) -> f64 {
    match (f_forward, f_backward, g_forward, g_backward) {
        (Some(ff), Some(fb), Some(gf), Some(gb)) => ff.max(fb).max(gf + gb),
        (Some(ff), None, Some(gf), _) => ff.max(gf),
        (None, Some(fb), _, Some(gb)) => fb.max(gb),
        _ => f64::INFINITY,
    }
}
