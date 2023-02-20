use k8s_openapi::api::core::v1::{Node, Pod};

pub(crate) fn is_pod_allocatable(node: &Node, pod: &Pod) -> bool {
    let Some(status) = &node.status else { return false };
    let Some(allocatable) = &status.allocatable else { return false };

    if let Some(annotations) = &node.metadata.annotations {
        //
    };

    // TODO: Implement allocatable filtering

    true
}

pub(crate) fn is_node_schedulable(node: &Node) -> bool {
    let Some(spec) = &node.spec else { return false };
    !spec.unschedulable.unwrap_or(false)
}

pub(crate) fn is_pod_taint_toleration_fulfilled(node: &Node, pod: &Pod) -> bool {
    let Some(spec) = &node.spec else { return false };
    // TODO: Implement taint filtering

    true
}

#[cfg(test)]
mod tests {
    use k8s_openapi::api::core::v1::NodeSpec;

    use super::*;

    #[test]
    fn test_schedulable_node() {
        assert!(is_node_schedulable(&Node {
            spec: Some(NodeSpec {
                ..Default::default()
            }),
            ..Default::default()
        }));
    }

    #[test]
    fn test_unschedulable_node_explicit() {
        assert!(!is_node_schedulable(&Node {
            spec: Some(NodeSpec {
                unschedulable: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        }));
    }

    #[test]
    fn test_unschedulable_node_empty() {
        assert!(!is_node_schedulable(&Node::default()));
    }
}
