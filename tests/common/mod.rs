use k8s_openapi::api::core::v1::{
    Container, ContainerStatus, Pod, PodSpec, PodStatus, Probe,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

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
