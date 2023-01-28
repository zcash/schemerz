use daggy::{
    petgraph::{
        visit::{GraphRef, IntoNeighborsDirected, VisitMap, Visitable},
        Direction,
    },
    Walker,
};

/// Just like `daggy::DfsPostOrder` but can traverse in either direction
#[derive(Clone, Debug)]
pub struct DfsPostOrderDirectional<N, VM> {
    direction: Direction,
    /// The stack of nodes to visit
    stack: Vec<N>,
    /// The map of discovered nodes
    discovered: VM,
    /// The map of finished nodes
    finished: VM,
}

impl<N, VM> DfsPostOrderDirectional<N, VM>
where
    N: Copy + PartialEq,
    VM: VisitMap<N>,
{
    /// Create a new `DfsPostOrderDirectional` using the graph's visitor map, and put
    /// `start` in the stack of nodes to visit.
    pub fn new<G>(direction: Direction, graph: G, start: N) -> Self
    where
        G: GraphRef + Visitable<NodeId = N, Map = VM>,
    {
        DfsPostOrderDirectional {
            direction,
            stack: vec![start],
            discovered: graph.visit_map(),
            finished: graph.visit_map(),
        }
    }

    /// Return the next node in the traversal, or `None` if the traversal is done.
    pub fn next<G>(&mut self, graph: G) -> Option<N>
    where
        G: IntoNeighborsDirected<NodeId = N>,
    {
        while let Some(&nx) = self.stack.last() {
            if self.discovered.visit(nx) {
                // First time visiting `nx`: Push neighbors, don't pop `nx`
                for succ in graph.neighbors_directed(nx, self.direction) {
                    if !self.discovered.is_visited(&succ) {
                        self.stack.push(succ);
                    }
                }
            } else {
                self.stack.pop();
                if self.finished.visit(nx) {
                    // Second time: All reachable nodes must have been finished
                    return Some(nx);
                }
            }
        }
        None
    }
}

impl<G> Walker<G> for DfsPostOrderDirectional<G::NodeId, G::Map>
where
    G: IntoNeighborsDirected + Visitable,
{
    type Item = G::NodeId;
    fn walk_next(&mut self, context: G) -> Option<Self::Item> {
        self.next(context)
    }
}
