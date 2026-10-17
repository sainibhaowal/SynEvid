use autopsy_incidents::{Deployment, IncidentWindow, correlate};

#[test]
fn correlates_in_window() {
    let deps = vec![
        Deployment {
            commit: "aaa".into(),
            timestamp_ms: 100,
            environment: "prod".into(),
        },
        Deployment {
            commit: "bbb".into(),
            timestamp_ms: 200,
            environment: "prod".into(),
        },
        Deployment {
            commit: "ccc".into(),
            timestamp_ms: 500,
            environment: "prod".into(),
        },
    ];
    let w = IncidentWindow {
        start_ms: 150,
        end_ms: 250,
        service: "svc".into(),
    };
    let r = correlate(&deps, &w).unwrap();
    assert_eq!(r.candidates.len(), 1);
    assert_eq!(r.candidates[0].deployment.commit, "bbb");
    assert_eq!(r.coverage, autopsy_domain::CoverageState::Partial);
}

#[test]
fn empty_deployments_unknown() {
    let w = IncidentWindow {
        start_ms: 0,
        end_ms: 10,
        service: "svc".into(),
    };
    let r = correlate(&[], &w).unwrap();
    assert_eq!(r.coverage, autopsy_domain::CoverageState::Unknown);
}

#[test]
fn invalid_window_errors() {
    let w = IncidentWindow {
        start_ms: 10,
        end_ms: 5,
        service: "svc".into(),
    };
    assert!(correlate(&[], &w).is_err());
}
