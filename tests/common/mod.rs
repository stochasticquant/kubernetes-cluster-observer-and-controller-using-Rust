use k8s_openapi::api::core::v1::{
    Container, ContainerStatus, Pod, PodSpec, PodStatus, Probe,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};

#[allow(dead_code)]
pub fn make_test_pod(
    name: &str,
    namespace: &str,
    image: &str,
    has_liveness: bool,
    has_readiness: bool,
    restart_count: i32,
    phase: &str,
) -> Pod {
    let probes = |has: bool| -> Option<Probe> {
        if has { Some(Probe::default()) } else { None }
    };

    Pod {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            namespace: Some(namespace.to_string()),
            ..Default::default()
        },
        spec: Some(PodSpec {
            containers: vec![Container {
                name: "main".to_string(),
                image: Some(image.to_string()),
                liveness_probe: probes(has_liveness),
                readiness_probe: probes(has_readiness),
                ..Default::default()
            }],
            ..Default::default()
        }),
        status: Some(PodStatus {
            phase: Some(phase.to_string()),
            container_statuses: Some(vec![ContainerStatus {
                name: "main".to_string(),
                restart_count,
                ready: phase == "Running",
                image: image.to_string(),
                image_id: String::new(),
                ..Default::default()
            }]),
            ..Default::default()
        }),
    }
}

/// Create a test pod that has an owner reference (Deployment, ReplicaSet, etc).
#[allow(dead_code)]
pub fn make_test_pod_with_owner(
    name: &str,
    namespace: &str,
    image: &str,
    owner_kind: &str,
    owner_name: &str,
    has_liveness: bool,
    has_readiness: bool,
) -> Pod {
    let probes = |has: bool| -> Option<Probe> {
        if has { Some(Probe::default()) } else { None }
    };

    Pod {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            namespace: Some(namespace.to_string()),
            owner_references: Some(vec![OwnerReference {
                kind: owner_kind.to_string(),
                name: owner_name.to_string(),
                api_version: "apps/v1".to_string(),
                uid: "test-uid".to_string(),
                ..Default::default()
            }]),
            ..Default::default()
        },
        spec: Some(PodSpec {
            containers: vec![Container {
                name: "main".to_string(),
                image: Some(image.to_string()),
                liveness_probe: probes(has_liveness),
                readiness_probe: probes(has_readiness),
                ..Default::default()
            }],
            ..Default::default()
        }),
        status: Some(PodStatus {
            phase: Some("Running".to_string()),
            container_statuses: Some(vec![ContainerStatus {
                name: "main".to_string(),
                restart_count: 0,
                ready: true,
                image: image.to_string(),
                image_id: String::new(),
                ..Default::default()
            }]),
            ..Default::default()
        }),
    }
}
