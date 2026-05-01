// Integration tests for all Ferroclaw Wave 1-3 features
// Tests feature interactions and end-to-end workflows

use ferroclaw::modes::plan::{PlanMode, PlanPhase};
use ferroclaw::tasks::{TaskCreate, TaskFilter, TaskStatus, TaskStore};
use tempfile::TempDir;

#[test]
fn test_task_store_and_plan_mode_integration() {
    // Test TaskStore with PlanMode integration
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create database file path
    let db_path = temp_dir.path().join("tasks.db");
    let task_store = TaskStore::new(Some(db_path)).unwrap();
    let plan_db_path = temp_dir.path().join("plan.db");
    let mut plan_mode = PlanMode::new(Some(plan_db_path)).unwrap();

    // Verify initial phase
    assert_eq!(plan_mode.phase(), PlanPhase::Research);

    // Create tasks with proper API
    let task1 = task_store
        .create(TaskCreate { subject: "Design component".to_string(), description: "Design the main component architecture".to_string(), active_form: None, owner: None, blocks: vec![], blocked_by: vec![], metadata: std::collections::HashMap::new() })
        .unwrap();

    let task1_id = task1.id.clone();

    let task2 = task_store
        .create(TaskCreate { subject: "Implement component".to_string(), description: "Implement the main component".to_string(), active_form: None, owner: None, blocks: vec![], blocked_by: vec![task1_id], metadata: std::collections::HashMap::new() })
        .unwrap();

    // Simulate plan progression
    plan_mode.approve_phase(None).unwrap();
    plan_mode.transition_phase(None).unwrap();
    assert_eq!(plan_mode.phase(), PlanPhase::Planning);

    // Update task statuses
    task_store
        .set_status(&task1.id, TaskStatus::Completed)
        .unwrap();
    task_store
        .set_status(&task2.id, TaskStatus::Completed)
        .unwrap();

    // Verify tasks completed
    let filter = TaskFilter {
        status: Some(TaskStatus::Completed),
        owner: None,
        blocked_by: None,
    };
    let completed_tasks = task_store.list(Some(filter)).unwrap();
    assert_eq!(completed_tasks.len(), 2);
}

#[test]
fn test_task_dependency_workflow() {
    // Test task dependency resolution
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("tasks.db");
    let task_store = TaskStore::new(Some(db_path)).unwrap();

    // Create task chain: A -> B -> C
    let task_a = task_store
        .create(TaskCreate { subject: "Task A".to_string(), description: "First task".to_string(), active_form: None, owner: None, blocks: vec![], blocked_by: vec![], metadata: std::collections::HashMap::new() })
        .unwrap();

    let task_b = task_store
        .create(TaskCreate { subject: "Task B".to_string(), description: "Second task depends on A".to_string(), active_form: None, owner: None, blocks: vec![], blocked_by: vec![task_a.id.clone()], metadata: std::collections::HashMap::new() })
        .unwrap();

    let task_c = task_store
        .create(TaskCreate { subject: "Task C".to_string(), description: "Third task depends on B".to_string(), active_form: None, owner: None, blocks: vec![], blocked_by: vec![task_b.id.clone()], metadata: std::collections::HashMap::new() })
        .unwrap();

    // Verify dependencies
    let filter = TaskFilter {
        status: Some(TaskStatus::Pending),
        owner: None,
        blocked_by: None,
    };
    let tasks = task_store.list(Some(filter)).unwrap();
    assert_eq!(tasks.len(), 3);

    // Complete tasks in order
    task_store
        .set_status(&task_a.id, TaskStatus::Completed)
        .unwrap();
    task_store
        .set_status(&task_b.id, TaskStatus::Completed)
        .unwrap();
    task_store
        .set_status(&task_c.id, TaskStatus::Completed)
        .unwrap();

    // Verify all completed
    let completed_filter = TaskFilter {
        status: Some(TaskStatus::Completed),
        owner: None,
        blocked_by: None,
    };
    let completed_tasks = task_store.list(Some(completed_filter)).unwrap();
    assert_eq!(completed_tasks.len(), 3);
}

#[test]
fn test_plan_mode_phase_progression() {
    // Test PlanMode phase progression
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let plan_db_path = temp_dir.path().join("plan.db");
    let mut plan_mode = PlanMode::new(Some(plan_db_path)).unwrap();

    // Start in Research phase
    assert_eq!(plan_mode.phase(), PlanPhase::Research);

    // Advance through phases (approving each phase before transition)
    plan_mode.approve_phase(None).unwrap();
    plan_mode.transition_phase(None).unwrap();
    assert_eq!(plan_mode.phase(), PlanPhase::Planning);

    plan_mode.approve_phase(None).unwrap();
    plan_mode.transition_phase(None).unwrap();
    assert_eq!(plan_mode.phase(), PlanPhase::Implementation);

    plan_mode.approve_phase(None).unwrap();
    plan_mode.transition_phase(None).unwrap();
    assert_eq!(plan_mode.phase(), PlanPhase::Verification);

    // Cannot advance beyond Verification
    let result = plan_mode.transition_phase(None);
    assert!(result.is_err());
}

#[test]
fn test_complete_workflow_simulation() {
    // Simulate a complete workflow using multiple features
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Initialize systems
    let task_db_path = temp_dir.path().join("tasks.db");
    let task_store = TaskStore::new(Some(task_db_path)).unwrap();
    let plan_db_path = temp_dir.path().join("plan.db");
    let mut plan_mode = PlanMode::new(Some(plan_db_path)).unwrap();

    // Verify initial phase
    assert_eq!(plan_mode.phase(), PlanPhase::Research);

    // Create tasks
    let design_task = task_store
        .create(TaskCreate { subject: "Design Feature X".to_string(), description: "Create design document".to_string(), active_form: None, owner: None, blocks: vec![], blocked_by: vec![], metadata: std::collections::HashMap::new() })
        .unwrap();

    let impl_task = task_store
        .create(TaskCreate { subject: "Implement Feature X".to_string(), description: "Write the code".to_string(), active_form: None, owner: None, blocks: vec![], blocked_by: vec![design_task.id.clone()], metadata: std::collections::HashMap::new() })
        .unwrap();

    // Advance plan
    plan_mode.approve_phase(None).unwrap();
    plan_mode.transition_phase(None).unwrap();
    assert_eq!(plan_mode.phase(), PlanPhase::Planning);

    // Update task statuses
    task_store
        .set_status(&design_task.id, TaskStatus::Completed)
        .unwrap();
    task_store
        .set_status(&impl_task.id, TaskStatus::InProgress)
        .unwrap();

    // Verify workflow state
    let pending_filter = TaskFilter {
        status: Some(TaskStatus::Pending),
        owner: None,
        blocked_by: None,
    };
    let all_tasks = task_store.list(Some(pending_filter)).unwrap();

    let in_progress_filter = TaskFilter {
        status: Some(TaskStatus::InProgress),
        owner: None,
        blocked_by: None,
    };
    let in_progress_tasks = task_store.list(Some(in_progress_filter)).unwrap();

    let completed_filter = TaskFilter {
        status: Some(TaskStatus::Completed),
        owner: None,
        blocked_by: None,
    };
    let completed_tasks = task_store.list(Some(completed_filter)).unwrap();

    assert_eq!(in_progress_tasks.len(), 1);
    assert_eq!(completed_tasks.len(), 1);
    assert_eq!(all_tasks.len(), 0); // All tasks are either in progress or completed
}
