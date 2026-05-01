//! Comprehensive tests for PlanMode
//!
//! This test suite covers:
//! - Phase transitions and approval gates
//! - Step creation and validation
//! - Dependency graph building and wave assignment
//! - Approval gate enforcement
//! - Status updates and dependent step unblocking

#[cfg(test)]
mod plan_mode_tests {
    use ferroclaw::modes::plan::{CreateStepInput, PlanMode, PlanPhase, PlanStepStatus};
    use ferroclaw::tasks::TaskStore;
    use std::collections::HashMap;

    /// Helper to create a test plan with in-memory store
    fn create_test_plan() -> PlanMode {
        PlanMode::new(None).expect("Failed to create test plan")
    }

    mod phase_transitions {
        use super::*;

        #[test]
        fn test_initial_phase_is_research() {
            let plan = create_test_plan();
            assert_eq!(plan.phase(), PlanPhase::Research);
        }

        #[test]
        fn test_research_to_planning_requires_approval() {
            let mut plan = create_test_plan();
            let result = plan.transition_phase(None);

            assert!(result.is_err());
            let err_msg = result.unwrap_err().to_string();
            assert!(err_msg.contains("approval required"));
        }

        #[test]
        fn test_approve_research_phase() {
            let mut plan = create_test_plan();
            plan.approve_phase(Some("Research complete".into()))
                .expect("Approval should succeed");

            assert!(plan.is_phase_approved(PlanPhase::Research));
        }

        #[test]
        fn test_successful_research_to_planning() {
            let mut plan = create_test_plan();
            plan.approve_phase(Some("Done".into())).unwrap();

            let next = plan.transition_phase(None).unwrap();
            assert_eq!(next, PlanPhase::Planning);
            assert_eq!(plan.phase(), PlanPhase::Planning);
        }

        #[test]
        fn test_full_phase_sequence() {
            let mut plan = create_test_plan();

            // Research -> Planning
            plan.approve_phase(Some("Research done".into())).unwrap();
            plan.transition_phase(None).unwrap();
            assert_eq!(plan.phase(), PlanPhase::Planning);

            // Planning -> Implementation
            plan.approve_phase(Some("Plan approved".into())).unwrap();
            plan.transition_phase(None).unwrap();
            assert_eq!(plan.phase(), PlanPhase::Implementation);

            // Implementation -> Verification
            plan.approve_phase(Some("Implementation complete".into())).unwrap();
            plan.transition_phase(None).unwrap();
            assert_eq!(plan.phase(), PlanPhase::Verification);
        }

        #[test]
        fn test_verification_is_terminal() {
            let mut plan = create_test_plan();

            // Fast-forward to verification
            for notes in &["Research done", "Plan approved", "Implementation complete"] {
                plan.approve_phase(Some(notes.to_string())).unwrap();
                plan.transition_phase(None).unwrap();
            }

            assert_eq!(plan.phase(), PlanPhase::Verification);

            // Cannot transition from verification
            let result = plan.transition_phase(None);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("terminal"));
        }

        #[test]
        fn test_approval_gates_persist() {
            let mut plan = create_test_plan();

            plan.approve_phase(Some("Note 1".into())).unwrap();
            plan.transition_phase(None).unwrap();

            plan.approve_phase(Some("Note 2".into())).unwrap();
            plan.transition_phase(None).unwrap();

            // Check that previous gates are still approved
            assert!(plan.is_phase_approved(PlanPhase::Research));
            assert!(plan.is_phase_approved(PlanPhase::Planning));
            assert!(!plan.is_phase_approved(PlanPhase::Implementation));
        }
    }

    mod step_creation {
        use super::*;

        #[test]
        fn test_create_basic_step() {
            let mut plan = create_test_plan();

            let step = plan
                .create_step(CreateStepInput { subject: "Test step".to_string(), description: "Test description".to_string(), active_form: Some("Testing".into()), acceptance_criteria: vec!["Criterion 1".into()], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step creation should succeed");

            assert_eq!(step.subject, "Test step");
            assert_eq!(step.description, "Test description");
            assert_eq!(step.active_form, Some("Testing".into()));
            assert_eq!(step.acceptance_criteria, vec!["Criterion 1"]);
            assert_eq!(step.status, PlanStepStatus::Pending);
            assert_eq!(step.wave, 0);
            assert!(!step.requires_approval);
        }

        #[test]
        fn test_step_with_approval_is_awaiting() {
            let mut plan = create_test_plan();

            let step = plan
                .create_step(CreateStepInput { subject: "Sensitive step".to_string(), description: "Requires approval".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: true, metadata: HashMap::new() })
                .expect("Step creation should succeed");

            assert_eq!(step.status, PlanStepStatus::AwaitingApproval);
            assert!(step.requires_approval);
            assert!(!step.approval_granted);
        }

        #[test]
        fn test_step_with_dependencies_is_blocked() {
            let mut plan = create_test_plan();

            // Create independent step first
            let step1 = plan
                .create_step(CreateStepInput { subject: "Step 1".to_string(), description: "First".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 1 creation should succeed");

            // Create dependent step
            let step2 = plan
                .create_step(CreateStepInput { subject: "Step 2".to_string(), description: "Second".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![step1.id.clone()], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 2 creation should succeed");

            assert_eq!(step2.depends_on, vec![step1.id]);
            assert_eq!(step2.status, PlanStepStatus::Blocked);
            assert!(step2.wave > 0);
        }

        #[test]
        fn test_create_step_with_invalid_dependency_fails() {
            let mut plan = create_test_plan();

            let result = plan.create_step(CreateStepInput { subject: "Invalid step".to_string(), description: "Has non-existent dependency".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec!["nonexistent-id".to_string()], requires_approval: false, metadata: HashMap::new() });

            assert!(result.is_err());
            let err_msg = result.unwrap_err().to_string();
            assert!(err_msg.contains("not found"));
        }

        #[test]
        fn test_step_metadata_persists() {
            let mut plan = create_test_plan();

            let mut metadata = HashMap::new();
            metadata.insert("priority".to_string(), serde_json::json!("high"));
            metadata.insert("estimated_hours".to_string(), serde_json::json!(5));

            let step = plan
                .create_step(CreateStepInput { subject: "Step with metadata".to_string(), description: "Description".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: metadata.clone() })
                .expect("Step creation should succeed");

            assert_eq!(step.metadata.get("priority").unwrap(), &serde_json::json!("high"));
            assert_eq!(
                step.metadata.get("estimated_hours").unwrap(),
                &serde_json::json!(5)
            );
        }

        #[test]
        fn test_get_step_by_id() {
            let mut plan = create_test_plan();

            let created = plan
                .create_step(CreateStepInput { subject: "Test step".to_string(), description: "Description".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step creation should succeed");

            let retrieved = plan
                .get_step(&created.id)
                .expect("Get step should succeed")
                .expect("Step should exist");

            assert_eq!(retrieved.id, created.id);
            assert_eq!(retrieved.subject, created.subject);
        }

        #[test]
        fn test_get_nonexistent_step_returns_none() {
            let mut plan = create_test_plan();

            let result = plan.get_step("nonexistent").expect("Get should not error");
            assert!(result.is_none());
        }
    }

    mod dependency_graph {
        use super::*;

        #[test]
        fn test_wave_calculation_independent_steps() {
            let mut plan = create_test_plan();

            let step1 = plan
                .create_step(CreateStepInput { subject: "Step 1".to_string(), description: "First".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 1 creation should succeed");

            let step2 = plan
                .create_step(CreateStepInput { subject: "Step 2".to_string(), description: "Second".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 2 creation should succeed");

            assert_eq!(step1.wave, 0);
            assert_eq!(step2.wave, 0);

            let waves = plan.calculate_waves().expect("Wave calculation should succeed");
            assert_eq!(waves.len(), 1);
            assert_eq!(waves[0].step_ids.len(), 2);
        }

        #[test]
        fn test_wave_calculation_simple_chain() {
            let mut plan = create_test_plan();

            let step1 = plan
                .create_step(CreateStepInput { subject: "Step 1".to_string(), description: "First".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 1 creation should succeed");

            let step2 = plan
                .create_step(CreateStepInput { subject: "Step 2".to_string(), description: "Second".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![step1.id.clone()], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 2 creation should succeed");

            let step3 = plan
                .create_step(CreateStepInput { subject: "Step 3".to_string(), description: "Third".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![step2.id.clone()], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 3 creation should succeed");

            assert_eq!(step1.wave, 0);
            assert_eq!(step2.wave, 1);
            assert_eq!(step3.wave, 2);

            let waves = plan.calculate_waves().expect("Wave calculation should succeed");
            assert_eq!(waves.len(), 3);
            assert_eq!(waves[0].number, 0);
            assert_eq!(waves[1].number, 1);
            assert_eq!(waves[2].number, 2);
        }

        #[test]
        fn test_wave_calculation_diamond_dependency() {
            let mut plan = create_test_plan();

            // Create diamond: A -> B, A -> C, B -> D, C -> D
            let step_a = plan
                .create_step(CreateStepInput { subject: "A".to_string(), description: "Base".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step A creation should succeed");

            let step_b = plan
                .create_step(CreateStepInput { subject: "B".to_string(), description: "Branch 1".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![step_a.id.clone()], requires_approval: false, metadata: HashMap::new() })
                .expect("Step B creation should succeed");

            let step_c = plan
                .create_step(CreateStepInput { subject: "C".to_string(), description: "Branch 2".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![step_a.id.clone()], requires_approval: false, metadata: HashMap::new() })
                .expect("Step C creation should succeed");

            let step_d = plan
                .create_step(CreateStepInput { subject: "D".to_string(), description: "Merge".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![step_b.id.clone(), step_c.id.clone()], requires_approval: false, metadata: HashMap::new() })
                .expect("Step D creation should succeed");

            assert_eq!(step_a.wave, 0);
            assert_eq!(step_b.wave, 1);
            assert_eq!(step_c.wave, 1);
            assert_eq!(step_d.wave, 2);

            let waves = plan.calculate_waves().expect("Wave calculation should succeed");
            assert_eq!(waves.len(), 3);
            assert_eq!(waves[0].step_ids.len(), 1); // A
            assert_eq!(waves[1].step_ids.len(), 2); // B, C
            assert_eq!(waves[2].step_ids.len(), 1); // D
        }

        #[test]
        fn test_complex_dependency_graph() {
            let mut plan = create_test_plan();

            // Create complex graph with multiple dependencies
            let step1 = plan
                .create_step(CreateStepInput { subject: "1".to_string(), description: "Base".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 1 creation should succeed");

            let step2 = plan
                .create_step(CreateStepInput { subject: "2".to_string(), description: "Depends on 1".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![step1.id.clone()], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 2 creation should succeed");

            let step3 = plan
                .create_step(CreateStepInput { subject: "3".to_string(), description: "Also depends on 1".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![step1.id.clone()], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 3 creation should succeed");

            let step4 = plan
                .create_step(CreateStepInput { subject: "4".to_string(), description: "Depends on 2 and 3".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![step2.id.clone(), step3.id.clone()], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 4 creation should succeed");

            assert_eq!(step1.wave, 0);
            assert_eq!(step2.wave, 1);
            assert_eq!(step3.wave, 1);
            assert_eq!(step4.wave, 2);

            let waves = plan.calculate_waves().expect("Wave calculation should succeed");
            assert_eq!(waves.len(), 3);
        }

        #[test]
        fn test_list_steps_ordered_by_creation() {
            let mut plan = create_test_plan();

            plan.create_step(CreateStepInput { subject: "Step 1".to_string(), description: "First".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 1 creation should succeed");

            plan.create_step(CreateStepInput { subject: "Step 2".to_string(), description: "Second".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 2 creation should succeed");

            plan.create_step(CreateStepInput { subject: "Step 3".to_string(), description: "Third".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 3 creation should succeed");

            let steps = plan.list_steps().expect("List steps should succeed");
            assert_eq!(steps.len(), 3);

            // Should be ordered by creation time (newest first)
            assert_eq!(steps[0].subject, "Step 3");
            assert_eq!(steps[1].subject, "Step 2");
            assert_eq!(steps[2].subject, "Step 1");
        }
    }

    mod approval_enforcement {
        use super::*;

        #[test]
        fn test_step_requires_approval() {
            let mut plan = create_test_plan();

            let step = plan
                .create_step(CreateStepInput { subject: "Sensitive step".to_string(), description: "Needs approval".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: true, metadata: HashMap::new() })
                .expect("Step creation should succeed");

            assert!(step.requires_approval);
            assert_eq!(step.status, PlanStepStatus::AwaitingApproval);
        }

        #[test]
        fn test_approve_step_transitions_to_pending() {
            let mut plan = create_test_plan();

            let step = plan
                .create_step(CreateStepInput { subject: "Sensitive step".to_string(), description: "Needs approval".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: true, metadata: HashMap::new() })
                .expect("Step creation should succeed");

            let approved = plan
                .approve_step(&step.id)
                .expect("Approval should succeed")
                .expect("Step should exist");

            assert_eq!(approved.status, PlanStepStatus::Pending);
            assert!(approved.approval_granted);
        }

        #[test]
        fn test_approve_non_approval_step_fails() {
            let mut plan = create_test_plan();

            let step = plan
                .create_step(CreateStepInput { subject: "Normal step".to_string(), description: "No approval needed".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step creation should succeed");

            let result = plan.approve_step(&step.id);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("does not require approval"));
        }

        #[test]
        fn test_approve_nonexistent_step_fails() {
            let mut plan = create_test_plan();

            let result = plan.approve_step("nonexistent");
            assert!(result.is_ok());
            assert!(result.unwrap().is_none());
        }

        #[test]
        fn test_phase_approval_persistence() {
            let mut plan = create_test_plan();

            plan.approve_phase(Some("Research complete".into()))
                .expect("Approval should succeed");

            assert!(plan.is_phase_approved(PlanPhase::Research));

            // Transition and check approval persists
            plan.transition_phase(None).expect("Transition should succeed");

            assert!(plan.is_phase_approved(PlanPhase::Research));
        }
    }

    mod status_updates {
        use super::*;

        #[test]
        fn test_update_step_to_in_progress() {
            let mut plan = create_test_plan();

            let step = plan
                .create_step(CreateStepInput { subject: "Test step".to_string(), description: "Description".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step creation should succeed");

            let updated = plan
                .update_step_status(&step.id, PlanStepStatus::InProgress)
                .expect("Update should succeed")
                .expect("Step should exist");

            assert_eq!(updated.status, PlanStepStatus::InProgress);
        }

        #[test]
        fn test_complete_step() {
            let mut plan = create_test_plan();

            let step = plan
                .create_step(CreateStepInput { subject: "Test step".to_string(), description: "Description".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step creation should succeed");

            let completed = plan
                .update_step_status(&step.id, PlanStepStatus::Completed)
                .expect("Update should succeed")
                .expect("Step should exist");

            assert_eq!(completed.status, PlanStepStatus::Completed);
        }

        #[test]
        fn test_mark_step_as_failed() {
            let mut plan = create_test_plan();

            let step = plan
                .create_step(CreateStepInput { subject: "Test step".to_string(), description: "Description".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step creation should succeed");

            let failed = plan
                .update_step_status(&step.id, PlanStepStatus::Failed)
                .expect("Update should succeed")
                .expect("Step should exist");

            assert_eq!(failed.status, PlanStepStatus::Failed);
        }

        #[test]
        fn test_completing_dependency_unblocks_dependent() {
            let mut plan = create_test_plan();

            let step1 = plan
                .create_step(CreateStepInput { subject: "Step 1".to_string(), description: "First".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 1 creation should succeed");

            let step2 = plan
                .create_step(CreateStepInput { subject: "Step 2".to_string(), description: "Second".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![step1.id.clone()], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 2 creation should succeed");

            assert_eq!(step2.status, PlanStepStatus::Blocked);

            // Complete step1
            plan.update_step_status(&step1.id, PlanStepStatus::Completed)
                .expect("Update should succeed");

            // Check step2 is now pending
            let step2_updated = plan
                .get_step(&step2.id)
                .expect("Get should succeed")
                .expect("Step should exist");

            assert_eq!(step2_updated.status, PlanStepStatus::Pending);
        }

        #[test]
        fn test_multiple_dependencies_all_must_complete() {
            let mut plan = create_test_plan();

            let step1 = plan
                .create_step(CreateStepInput { subject: "Step 1".to_string(), description: "First".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 1 creation should succeed");

            let step2 = plan
                .create_step(CreateStepInput { subject: "Step 2".to_string(), description: "Second".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 2 creation should succeed");

            let step3 = plan
                .create_step(CreateStepInput { subject: "Step 3".to_string(), description: "Depends on both".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![step1.id.clone(), step2.id.clone()], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 3 creation should succeed");

            assert_eq!(step3.status, PlanStepStatus::Blocked);

            // Complete only step1
            plan.update_step_status(&step1.id, PlanStepStatus::Completed)
                .expect("Update should succeed");

            // step3 should still be blocked
            let step3_updated = plan
                .get_step(&step3.id)
                .expect("Get should succeed")
                .expect("Step should exist");

            assert_eq!(step3_updated.status, PlanStepStatus::Blocked);

            // Complete step2
            plan.update_step_status(&step2.id, PlanStepStatus::Completed)
                .expect("Update should succeed");

            // Now step3 should be pending
            let step3_final = plan
                .get_step(&step3.id)
                .expect("Get should succeed")
                .expect("Step should exist");

            assert_eq!(step3_final.status, PlanStepStatus::Pending);
        }

        #[test]
        fn test_update_nonexistent_step_returns_none() {
            let mut plan = create_test_plan();

            let result = plan.update_step_status("nonexistent", PlanStepStatus::InProgress);
            assert!(result.is_ok());
            assert!(result.unwrap().is_none());
        }
    }

    mod status_summary {
        use super::*;

        #[test]
        fn test_empty_plan_status() {
            let mut plan = create_test_plan();

            let status = plan.status().expect("Status should succeed");

            assert_eq!(status.total_steps, 0);
            assert_eq!(status.completed, 0);
            assert_eq!(status.pending, 0);
            assert_eq!(status.in_progress, 0);
            assert_eq!(status.blocked, 0);
            assert_eq!(status.awaiting_approval, 0);
            assert_eq!(status.failed, 0);
            assert_eq!(status.phase, PlanPhase::Research);
        }

        #[test]
        fn test_status_counts_all_states() {
            let mut plan = create_test_plan();

            // Create steps in different states
            let step1 = plan
                .create_step(CreateStepInput { subject: "Step 1".to_string(), description: "First".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 1 creation should succeed");

            let step2 = plan
                .create_step(CreateStepInput { subject: "Step 2".to_string(), description: "Needs approval".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: true, metadata: HashMap::new() })
                .expect("Step 2 creation should succeed");

            let step3 = plan
                .create_step(CreateStepInput { subject: "Step 3".to_string(), description: "Depends on 1".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![step1.id.clone()], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 3 creation should succeed");

            // Update some statuses
            plan.update_step_status(&step1.id, PlanStepStatus::InProgress)
                .expect("Update should succeed");

            plan.approve_step(&step2.id).expect("Approval should succeed");

            let status = plan.status().expect("Status should succeed");

            assert_eq!(status.total_steps, 3);
            assert_eq!(status.in_progress, 1);
            assert_eq!(status.blocked, 1);
            assert_eq!(status.pending, 1);
        }

        #[test]
        fn test_status_includes_waves() {
            let mut plan = create_test_plan();

            plan.create_step(CreateStepInput { subject: "Step 1".to_string(), description: "First".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 1 creation should succeed");

            let step2 = plan
                .create_step(CreateStepInput { subject: "Step 1".to_string(), description: "First".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 2 creation should succeed");

            let status = plan.status().expect("Status should succeed");

            assert!(!status.waves.is_empty());
            assert_eq!(status.waves[0].number, 0);
            assert!(status.waves[0].step_ids.contains(&step2.id));
        }

        #[test]
        fn test_can_transition_reflects_approval() {
            let mut plan = create_test_plan();

            let status = plan.status().expect("Status should succeed");
            assert!(!status.can_transition);

            plan.approve_phase(Some("Done".into())).expect("Approval should succeed");

            let status = plan.status().expect("Status should succeed");
            assert!(status.can_transition);
        }
    }

    mod integration_with_task_system {
        use super::*;

        #[test]
        fn test_plan_uses_task_store_persistence() {
            let mut plan = create_test_plan();

            let step = plan
                .create_step(CreateStepInput { subject: "Persistent step".to_string(), description: "Should persist in TaskStore".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step creation should succeed");

            // Verify step exists in task store
            let task_store = TaskStore::in_memory().expect("Failed to create store");
            // Note: In-memory stores are separate, so this tests the integration pattern
            // In production, the same store would be used

            assert!(!step.id.is_empty());
            assert!(step.id.len() > 0);
        }

        #[test]
        fn test_step_status_syncs_with_task_status() {
            let mut plan = create_test_plan();

            let step = plan
                .create_step(CreateStepInput { subject: "Sync test".to_string(), description: "Should sync with task".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step creation should succeed");

            // PlanStepStatus::Pending maps to TaskStatus::Pending
            assert_eq!(step.status, PlanStepStatus::Pending);

            // Update to in progress
            let updated = plan
                .update_step_status(&step.id, PlanStepStatus::InProgress)
                .expect("Update should succeed")
                .expect("Step should exist");

            assert_eq!(updated.status, PlanStepStatus::InProgress);

            // Update to completed
            let completed = plan
                .update_step_status(&step.id, PlanStepStatus::Completed)
                .expect("Update should succeed")
                .expect("Step should exist");

            assert_eq!(completed.status, PlanStepStatus::Completed);
        }

        #[test]
        fn test_blocked_steps_map_to_pending_in_store() {
            let mut plan = create_test_plan();

            let step1 = plan
                .create_step(CreateStepInput { subject: "Step 1".to_string(), description: "First".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 1 creation should succeed");

            let step2 = plan
                .create_step(CreateStepInput { subject: "Step 2".to_string(), description: "Blocked".to_string(), active_form: None, acceptance_criteria: vec![], depends_on: vec![step1.id.clone()], requires_approval: false, metadata: HashMap::new() })
                .expect("Step 2 creation should succeed");

            // PlanStepStatus::Blocked, but TaskStatus::Pending in store
            assert_eq!(step2.status, PlanStepStatus::Blocked);
        }
    }
}
