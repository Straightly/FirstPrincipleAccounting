//! M5 engine additions: workflow deployment, auto-roles, role assignment,
//! and workflow-scoped authorization + execution-context verification on
//! `post_entry` (Impl Spec §2.9, §6.1, §6.5, §4.1.5).

mod common;

use common::*;
use ledgerzero_engine::domain::*;
use ledgerzero_engine::engine::*;
use ledgerzero_engine::{EngineState, ErrorCode};
use serde_json::Value;
use uuid::Uuid;

fn deploy_startup_expense(fx: &mut Fx) -> (Uuid, Uuid) {
    let deployment_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let outcome = fx
        .engine
        .deploy_workflow(
            fx.actor,
            NewWorkflowDeployment {
                workflow_deployment_id: deployment_id,
                workflow_id,
                entity_id: fx.entity,
                workflow_name: "Recording startup expense".into(),
                description: Some("Hand-built reference workflow".into()),
                artifact_id: deployment_id,
                dev_artifact_path: format!("dev_artifacts/workflows/{deployment_id}"),
                manifest_hash: "manifest-hash".into(),
                code_hash: "code-hash".into(),
                frontend_route: format!("/workflows/{deployment_id}/index.html"),
                backend_api_calls: vec!["post_entry".into()],
                required_inputs: Value::Null,
                metadata: Value::Null,
            },
        )
        .unwrap();
    assert_eq!(outcome, deployment_id);
    let definition = fx.engine.get_workflow(deployment_id).unwrap();
    assert_eq!(definition.workflow_id, workflow_id);
    (deployment_id, definition.workflow_id)
}

#[test]
fn deploy_workflow_auto_creates_a_role_with_exactly_that_workflow() {
    let mut fx = fixture();
    let (deployment_id, workflow_id) = deploy_startup_expense(&mut fx);

    let workflows = fx.engine.list_workflows(fx.entity);
    assert_eq!(workflows.len(), 1);
    assert_eq!(workflows[0].workflow_deployment_id, deployment_id);
    assert_eq!(workflows[0].backend_api_calls, vec!["post_entry"]);

    let roles = fx.engine.list_roles(fx.entity);
    assert_eq!(roles.len(), 1, "deploying a workflow auto-creates one role");
    assert_eq!(roles[0].name, "Recording startup expense");
    assert_eq!(roles[0].workflow_ids, vec![workflow_id]);
}

#[test]
fn deploy_workflow_is_idempotent_and_conflicts_on_tamper() {
    let mut fx = fixture();
    let deployment_id = Uuid::new_v4();
    let spec = NewWorkflowDeployment {
        workflow_deployment_id: deployment_id,
        workflow_id: Uuid::new_v4(),
        entity_id: fx.entity,
        workflow_name: "Recording startup expense".into(),
        description: None,
        artifact_id: deployment_id,
        dev_artifact_path: "dev_artifacts/workflows/x".into(),
        manifest_hash: "m".into(),
        code_hash: "c".into(),
        frontend_route: "/workflows/x/index.html".into(),
        backend_api_calls: vec!["post_entry".into()],
        required_inputs: Value::Null,
        metadata: Value::Null,
    };
    let first = fx.engine.deploy_workflow(fx.actor, spec.clone()).unwrap();
    let log_len = fx.engine.audit_log().len();

    let replay = fx.engine.deploy_workflow(fx.actor, spec.clone()).unwrap();
    assert_eq!(replay, first);
    assert_eq!(
        fx.engine.audit_log().len(),
        log_len,
        "replay appends nothing"
    );

    let mut tampered = spec;
    tampered.code_hash = "different".into();
    let err = fx.engine.deploy_workflow(fx.actor, tampered).unwrap_err();
    assert_eq!(err.error_code, ErrorCode::IdempotencyConflict);
}

#[test]
fn deploy_workflow_rejects_name_collision_with_existing_workflow_or_role() {
    let mut fx = fixture();
    deploy_startup_expense(&mut fx);

    let collide_workflow = NewWorkflowDeployment {
        workflow_deployment_id: Uuid::new_v4(),
        workflow_id: Uuid::new_v4(),
        entity_id: fx.entity,
        workflow_name: "Recording startup expense".into(),
        description: None,
        artifact_id: Uuid::new_v4(),
        dev_artifact_path: "x".into(),
        manifest_hash: "m2".into(),
        code_hash: "c2".into(),
        frontend_route: "/workflows/y/index.html".into(),
        backend_api_calls: vec!["post_entry".into()],
        required_inputs: Value::Null,
        metadata: Value::Null,
    };
    let err = fx
        .engine
        .deploy_workflow(fx.actor, collide_workflow)
        .unwrap_err();
    assert_eq!(err.error_code, ErrorCode::InvalidInput);

    // A pre-existing role with the same name also collides with the
    // would-be auto-role.
    fx.engine
        .create_role(
            id(),
            fx.actor,
            NewRole {
                entity_id: fx.entity,
                name: "Manually named role".into(),
                description: None,
            },
        )
        .unwrap();
    let collide_role = NewWorkflowDeployment {
        workflow_deployment_id: Uuid::new_v4(),
        workflow_id: Uuid::new_v4(),
        entity_id: fx.entity,
        workflow_name: "Manually named role".into(),
        description: None,
        artifact_id: Uuid::new_v4(),
        dev_artifact_path: "z".into(),
        manifest_hash: "m3".into(),
        code_hash: "c3".into(),
        frontend_route: "/workflows/z/index.html".into(),
        backend_api_calls: vec!["post_entry".into()],
        required_inputs: Value::Null,
        metadata: Value::Null,
    };
    let err = fx
        .engine
        .deploy_workflow(fx.actor, collide_role)
        .unwrap_err();
    assert_eq!(err.error_code, ErrorCode::InvalidInput);
}

#[test]
fn create_role_assign_workflow_assign_user_round_trip() {
    let mut fx = fixture();
    let (_, workflow_id) = deploy_startup_expense(&mut fx);
    let other_user = Uuid::new_v4();

    let role_id = fx
        .engine
        .create_role(
            id(),
            fx.actor,
            NewRole {
                entity_id: fx.entity,
                name: "Composite Role".into(),
                description: Some("bundles workflows".into()),
            },
        )
        .unwrap();
    assert!(fx
        .engine
        .list_roles(fx.entity)
        .iter()
        .any(|r| r.role_id == role_id && r.workflow_ids.is_empty()));

    fx.engine
        .assign_workflow_to_role(id(), fx.actor, role_id, workflow_id)
        .unwrap();
    assert_eq!(
        fx.engine.get_role(role_id).unwrap().workflow_ids,
        vec![workflow_id]
    );

    // Duplicate assignment rejected.
    let err = fx
        .engine
        .assign_workflow_to_role(id(), fx.actor, role_id, workflow_id)
        .unwrap_err();
    assert_eq!(err.error_code, ErrorCode::InvalidInput);

    fx.engine
        .assign_role_to_user(id(), fx.actor, role_id, other_user)
        .unwrap();
    assert_eq!(fx.engine.list_role_assignments(role_id), vec![other_user]);

    let err = fx
        .engine
        .assign_role_to_user(id(), fx.actor, role_id, other_user)
        .unwrap_err();
    assert_eq!(err.error_code, ErrorCode::InvalidInput);

    assert!(fx
        .engine
        .workflows_authorized_for_user(other_user, fx.entity)
        .iter()
        .any(|w| w.workflow_id == workflow_id));
}

#[test]
fn unassigned_user_is_rejected_with_unauthorized_workflow() {
    let mut fx = fixture();
    let (deployment_id, workflow_id) = deploy_startup_expense(&mut fx);
    let stranger = Uuid::new_v4();

    let entry = fx.entry(
        "2026-02-10",
        "workflow expense",
        vec![debit(fx.rent, "40.00"), credit(fx.cash, "40.00")],
    );
    let mut entry = entry;
    entry.workflow = Some(WorkflowContext {
        workflow_id,
        workflow_deployment_id: deployment_id,
        workflow_execution_id: Uuid::new_v4(),
    });

    let before = fx.engine.state().clone();
    let err = fx.engine.post_entry(stranger, entry).unwrap_err();
    assert_eq!(err.error_code, ErrorCode::UnauthorizedWorkflow);
    assert_eq!(
        fx.engine.state(),
        &before,
        "rejection must not mutate state"
    );
}

#[test]
fn deployment_not_permitting_the_api_is_rejected_with_unauthorized_api() {
    let mut fx = fixture();
    // A deployment whose allow-list does not include post_entry.
    let deployment_id = Uuid::new_v4();
    fx.engine
        .deploy_workflow(
            fx.actor,
            NewWorkflowDeployment {
                workflow_deployment_id: deployment_id,
                workflow_id: Uuid::new_v4(),
                entity_id: fx.entity,
                workflow_name: "Read-only report".into(),
                description: None,
                artifact_id: deployment_id,
                dev_artifact_path: "dev_artifacts/workflows/report".into(),
                manifest_hash: "m".into(),
                code_hash: "c".into(),
                frontend_route: "/workflows/report/index.html".into(),
                backend_api_calls: vec!["get_balance".into()],
                required_inputs: Value::Null,
                metadata: Value::Null,
            },
        )
        .unwrap();
    let definition = fx.engine.get_workflow(deployment_id).unwrap();
    let workflow_id = definition.workflow_id;
    let role_id = fx
        .engine
        .list_roles(fx.entity)
        .into_iter()
        .find(|r| r.workflow_ids.contains(&workflow_id))
        .unwrap()
        .role_id;
    let user = Uuid::new_v4();
    fx.engine
        .assign_role_to_user(id(), fx.actor, role_id, user)
        .unwrap();

    let mut entry = fx.entry(
        "2026-02-10",
        "should be rejected",
        vec![debit(fx.rent, "10.00"), credit(fx.cash, "10.00")],
    );
    entry.workflow = Some(WorkflowContext {
        workflow_id,
        workflow_deployment_id: deployment_id,
        workflow_execution_id: Uuid::new_v4(),
    });

    let err = fx.engine.post_entry(user, entry).unwrap_err();
    assert_eq!(err.error_code, ErrorCode::UnauthorizedApi);
}

#[test]
fn mismatched_execution_context_is_rejected() {
    let mut fx = fixture();
    let (deployment_id, workflow_id) = deploy_startup_expense(&mut fx);
    let role_id = fx
        .engine
        .list_roles(fx.entity)
        .into_iter()
        .find(|r| r.workflow_ids.contains(&workflow_id))
        .unwrap()
        .role_id;
    let user = Uuid::new_v4();
    fx.engine
        .assign_role_to_user(id(), fx.actor, role_id, user)
        .unwrap();

    // Unknown deployment id.
    let mut bad_deployment = fx.entry(
        "2026-02-10",
        "x",
        vec![debit(fx.rent, "5.00"), credit(fx.cash, "5.00")],
    );
    bad_deployment.workflow = Some(WorkflowContext {
        workflow_id,
        workflow_deployment_id: Uuid::new_v4(),
        workflow_execution_id: Uuid::new_v4(),
    });
    let err = fx.engine.post_entry(user, bad_deployment).unwrap_err();
    assert_eq!(err.error_code, ErrorCode::InvalidExecutionContext);

    // workflow_id does not match the deployment's actual workflow_id.
    let mut bad_workflow_id = fx.entry(
        "2026-02-10",
        "x",
        vec![debit(fx.rent, "5.00"), credit(fx.cash, "5.00")],
    );
    bad_workflow_id.workflow = Some(WorkflowContext {
        workflow_id: Uuid::new_v4(),
        workflow_deployment_id: deployment_id,
        workflow_execution_id: Uuid::new_v4(),
    });
    let err = fx.engine.post_entry(user, bad_workflow_id).unwrap_err();
    assert_eq!(err.error_code, ErrorCode::InvalidExecutionContext);

    // Nil workflow_execution_id is structurally invalid.
    let mut nil_execution = fx.entry(
        "2026-02-10",
        "x",
        vec![debit(fx.rent, "5.00"), credit(fx.cash, "5.00")],
    );
    nil_execution.workflow = Some(WorkflowContext {
        workflow_id,
        workflow_deployment_id: deployment_id,
        workflow_execution_id: Uuid::nil(),
    });
    let err = fx.engine.post_entry(user, nil_execution).unwrap_err();
    assert_eq!(err.error_code, ErrorCode::InvalidInput);
}

#[test]
fn authorized_workflow_call_posts_and_preserves_context_on_the_entry() {
    let mut fx = fixture();
    let (deployment_id, workflow_id) = deploy_startup_expense(&mut fx);
    let role_id = fx
        .engine
        .list_roles(fx.entity)
        .into_iter()
        .find(|r| r.workflow_ids.contains(&workflow_id))
        .unwrap()
        .role_id;
    let employee = Uuid::new_v4();
    fx.engine
        .assign_role_to_user(id(), fx.actor, role_id, employee)
        .unwrap();

    let execution_id = Uuid::new_v4();
    let mut entry = fx.entry(
        "2026-02-10",
        "startup laptop expense",
        vec![debit(fx.rent, "899.00"), credit(fx.cash, "899.00")],
    );
    entry.workflow = Some(WorkflowContext {
        workflow_id,
        workflow_deployment_id: deployment_id,
        workflow_execution_id: execution_id,
    });
    let entry_id = entry.entry_id;

    // The book owner is NOT assigned this role and must also be rejected —
    // proving authorization is workflow-scoped, not blanket-owner.
    let mut owner_attempt = entry.clone();
    owner_attempt.entry_id = Uuid::new_v4();
    for line in &mut owner_attempt.lines {
        line.line_id = Uuid::new_v4();
    }
    let err = fx.engine.post_entry(fx.actor, owner_attempt).unwrap_err();
    assert_eq!(err.error_code, ErrorCode::UnauthorizedWorkflow);

    let posted_id = fx.engine.post_entry(employee, entry).unwrap();
    assert_eq!(posted_id, entry_id);

    let posted = fx.engine.get_entry(entry_id).unwrap();
    let ctx = posted.workflow.as_ref().unwrap();
    assert_eq!(ctx.workflow_id, workflow_id);
    assert_eq!(ctx.workflow_deployment_id, deployment_id);
    assert_eq!(ctx.workflow_execution_id, execution_id);
    assert_eq!(posted.posted_by, employee);

    let replayed = EngineState::replay(fx.book, fx.engine.audit_log()).unwrap();
    assert_eq!(
        &replayed,
        fx.engine.state(),
        "replay must match incremental state"
    );
}

#[test]
fn entities_with_workflows_for_user_backs_the_picker() {
    let mut fx = fixture();
    let (_, workflow_id) = deploy_startup_expense(&mut fx);
    let role_id = fx
        .engine
        .list_roles(fx.entity)
        .into_iter()
        .find(|r| r.workflow_ids.contains(&workflow_id))
        .unwrap()
        .role_id;
    let employee = Uuid::new_v4();
    let stranger = Uuid::new_v4();

    assert!(
        fx.engine
            .entities_with_workflows_for_user(employee)
            .is_empty(),
        "no role assignment yet: nothing to discover"
    );

    fx.engine
        .assign_role_to_user(id(), fx.actor, role_id, employee)
        .unwrap();

    assert_eq!(
        fx.engine.entities_with_workflows_for_user(employee),
        vec![fx.entity]
    );
    assert!(
        fx.engine
            .entities_with_workflows_for_user(stranger)
            .is_empty(),
        "a different user's assignment must not leak"
    );

    // A role with no workflows assigned yet must not count.
    let empty_role = fx
        .engine
        .create_role(
            id(),
            fx.actor,
            NewRole {
                entity_id: fx.entity,
                name: "Empty role".into(),
                description: None,
            },
        )
        .unwrap();
    fx.engine
        .assign_role_to_user(id(), fx.actor, empty_role, stranger)
        .unwrap();
    assert!(
        fx.engine
            .entities_with_workflows_for_user(stranger)
            .is_empty(),
        "a role granting zero workflows must not surface the entity"
    );
}
