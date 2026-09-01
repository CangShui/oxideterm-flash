use super::*;

pub(super) fn node_readiness_became_ready(
    previous: Option<&NodeReadiness>,
    current: &NodeReadiness,
) -> bool {
    !matches!(previous, Some(NodeReadiness::Ready)) && matches!(current, NodeReadiness::Ready)
}

pub(super) fn node_readiness_became_unavailable(
    previous: Option<&NodeReadiness>,
    current: &NodeReadiness,
) -> bool {
    !matches!(
        previous,
        Some(NodeReadiness::Error | NodeReadiness::Disconnected)
    ) && matches!(current, NodeReadiness::Error | NodeReadiness::Disconnected)
}

pub(super) fn reconnect_cascade_child_should_start(readiness: &NodeReadiness) -> bool {
    matches!(readiness, NodeReadiness::Error | NodeReadiness::Connecting)
}

#[cfg(test)]
mod node_reconnect_helper_tests {
    use super::*;

    #[test]
    fn ready_transition_requires_a_non_ready_previous_state() {
        assert!(node_readiness_became_ready(
            Some(&NodeReadiness::Connecting),
            &NodeReadiness::Ready
        ));
        assert!(node_readiness_became_ready(None, &NodeReadiness::Ready));
        assert!(!node_readiness_became_ready(
            Some(&NodeReadiness::Ready),
            &NodeReadiness::Ready
        ));
        assert!(!node_readiness_became_ready(
            Some(&NodeReadiness::Error),
            &NodeReadiness::Disconnected
        ));
        assert!(node_readiness_became_unavailable(
            Some(&NodeReadiness::Connecting),
            &NodeReadiness::Error
        ));
        assert!(node_readiness_became_unavailable(
            Some(&NodeReadiness::Ready),
            &NodeReadiness::Disconnected
        ));
        assert!(!node_readiness_became_unavailable(
            Some(&NodeReadiness::Error),
            &NodeReadiness::Disconnected
        ));
    }

    #[test]
    fn reconnect_cascade_skips_user_disconnected_children_like_tauri_link_down_set() {
        assert!(reconnect_cascade_child_should_start(&NodeReadiness::Error));
        assert!(reconnect_cascade_child_should_start(
            &NodeReadiness::Connecting
        ));
        assert!(!reconnect_cascade_child_should_start(
            &NodeReadiness::Disconnected
        ));
        assert!(!reconnect_cascade_child_should_start(&NodeReadiness::Ready));
    }
}
