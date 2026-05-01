//! Integration tests for TaskSystem

use crate::tasks::store::TaskStatus;
use crate::tasks::{TaskCreate, TaskFilter, TaskStore, TaskUpdate};
use std::collections::HashMap;

#[test]
fn test_task_creation_and_retrieval() {
    let store = TaskStore::in_memory().unwrap();

    // Create a task
    let task = store
        .create(TaskCreate {
            subject: "Implement feature".to_string(),
            description: "Build the new feature with full test coverage".to_string(),
            active_form: Some("Implementing".into()),
            owner: Some("agent1".into()),
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();

    assert!(!task.id.is_empty());
    assert_eq!(task.subject, "Implement feature");
    assert_eq!(
        task.description,
        "Build the new feature with full test coverage"
    );
    assert_eq!(task.active_form, Some("Implementing".into()));
    assert_eq!(task.status, TaskStatus::Pending);
    assert_eq!(task.owner, Some("agent1".into()));
    assert!(task.blocks.is_empty());
    assert!(task.blocked_by.is_empty());

    // Retrieve by ID
    let retrieved = store.get(&task.id).unwrap().unwrap();
    assert_eq!(retrieved.id, task.id);
    assert_eq!(retrieved.subject, task.subject);
    assert_eq!(retrieved.description, task.description);
}

#[test]
fn test_status_updates() {
    let store = TaskStore::in_memory().unwrap();

    let task = store
        .create(TaskCreate {
            subject: "Test task".to_string(),
            description: "Description".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();

    // Pending -> InProgress
    let updated = store
        .set_status(&task.id, TaskStatus::InProgress)
        .unwrap()
        .unwrap();
    assert_eq!(updated.status, TaskStatus::InProgress);

    // InProgress -> Completed
    let updated = store
        .set_status(&task.id, TaskStatus::Completed)
        .unwrap()
        .unwrap();
    assert_eq!(updated.status, TaskStatus::Completed);

    // Can go backwards too
    let updated = store
        .set_status(&task.id, TaskStatus::Pending)
        .unwrap()
        .unwrap();
    assert_eq!(updated.status, TaskStatus::Pending);
}

#[test]
fn test_dependency_tracking() {
    let store = TaskStore::in_memory().unwrap();

    // Create three tasks
    let task1 = store
        .create(TaskCreate {
            subject: "Task 1".to_string(),
            description: "First".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();
    let task2 = store
        .create(TaskCreate {
            subject: "Task 2".to_string(),
            description: "Second".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();
    let task3 = store
        .create(TaskCreate {
            subject: "Task 3".to_string(),
            description: "Third".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();

    // task1 blocks task2 (task2 depends on task1)
    let task1_updated = store.add_block(&task1.id, &task2.id).unwrap().unwrap();
    assert!(task1_updated.blocks.contains(&task2.id));

    // Verify reverse dependency was added
    let task2_updated = store.get(&task2.id).unwrap().unwrap();
    assert!(task2_updated.blocked_by.contains(&task1.id));

    // task1 also blocks task3
    store.add_block(&task1.id, &task3.id).unwrap().unwrap();

    // task1 should block both task2 and task3
    let task1_final = store.get(&task1.id).unwrap().unwrap();
    assert_eq!(task1_final.blocks.len(), 2);
    assert!(task1_final.blocks.contains(&task2.id));
    assert!(task1_final.blocks.contains(&task3.id));

    // Get tasks that task1 is blocking
    let blocked = store.get_blocked(&task1.id).unwrap();
    assert_eq!(blocked.len(), 2);
    let blocked_ids: Vec<String> = blocked.iter().map(|t| t.id.clone()).collect();
    assert!(blocked_ids.contains(&task2.id));
    assert!(blocked_ids.contains(&task3.id));

    // Get tasks that are blocking task2
    let blocking = store.get_blocking(&task2.id).unwrap();
    assert_eq!(blocking.len(), 1);
    assert_eq!(blocking[0].id, task1.id);

    // Remove dependency
    store.remove_block(&task1.id, &task2.id).unwrap().unwrap();
    let task1_after = store.get(&task1.id).unwrap().unwrap();
    assert!(!task1_after.blocks.contains(&task2.id));

    let task2_after = store.get(&task2.id).unwrap().unwrap();
    assert!(!task2_after.blocked_by.contains(&task1.id));
}

#[test]
fn test_cycle_detection_simple() {
    let store = TaskStore::in_memory().unwrap();

    let task1 = store
        .create(TaskCreate {
            subject: "Task 1".to_string(),
            description: "First".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();
    let task2 = store
        .create(TaskCreate {
            subject: "Task 2".to_string(),
            description: "Second".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();

    // task1 blocks task2
    store.add_block(&task1.id, &task2.id).unwrap();

    // Try to create reverse dependency (should fail)
    let result = store.add_block(&task2.id, &task1.id);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Cycle detected"));
}

#[test]
fn test_cycle_detection_complex() {
    let store = TaskStore::in_memory().unwrap();

    let task1 = store
        .create(TaskCreate {
            subject: "Task 1".to_string(),
            description: "First".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();
    let task2 = store
        .create(TaskCreate {
            subject: "Task 2".to_string(),
            description: "Second".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();
    let task3 = store
        .create(TaskCreate {
            subject: "Task 3".to_string(),
            description: "Third".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();
    let task4 = store
        .create(TaskCreate {
            subject: "Task 4".to_string(),
            description: "Fourth".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();

    // Create chain: task1 -> task2 -> task3 -> task4
    store.add_block(&task1.id, &task2.id).unwrap();
    store.add_block(&task2.id, &task3.id).unwrap();
    store.add_block(&task3.id, &task4.id).unwrap();

    // Try to create cycle: task4 -> task1 (should fail)
    let result = store.add_block(&task4.id, &task1.id);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Cycle detected"));

    // Try to create cycle: task4 -> task2 (should fail)
    let result = store.add_block(&task4.id, &task2.id);
    assert!(result.is_err());
}

#[test]
fn test_listing_with_filters() {
    let store = TaskStore::in_memory().unwrap();

    // Create tasks with different properties
    let task1 = store
        .create(TaskCreate {
            subject: "Task 1".to_string(),
            description: "First".to_string(),
            active_form: None,
            owner: Some("agent1".into()),
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();

    let task2 = store
        .create(TaskCreate {
            subject: "Task 2".to_string(),
            description: "Second".to_string(),
            active_form: None,
            owner: Some("agent2".into()),
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();

    let task3 = store
        .create(TaskCreate {
            subject: "Task 3".to_string(),
            description: "Third".to_string(),
            active_form: None,
            owner: Some("agent1".into()),
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();

    // Update task2 and task3 statuses
    store.set_status(&task2.id, TaskStatus::InProgress).unwrap();
    store.set_status(&task3.id, TaskStatus::Completed).unwrap();

    // List all tasks
    let all = store.list(None).unwrap();
    assert_eq!(all.len(), 3);

    // Filter by status: pending
    let pending_filter = TaskFilter {
        status: Some(TaskStatus::Pending),
        owner: None,
        blocked_by: None,
    };
    let pending = store.list(Some(pending_filter)).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, task1.id);

    // Filter by status: in_progress
    let in_progress_filter = TaskFilter {
        status: Some(TaskStatus::InProgress),
        owner: None,
        blocked_by: None,
    };
    let in_progress = store.list(Some(in_progress_filter)).unwrap();
    assert_eq!(in_progress.len(), 1);
    assert_eq!(in_progress[0].id, task2.id);

    // Filter by owner
    let owner_filter = TaskFilter {
        status: None,
        owner: Some("agent1".into()),
        blocked_by: None,
    };
    let agent1_tasks = store.list(Some(owner_filter)).unwrap();
    assert_eq!(agent1_tasks.len(), 2);
    let agent1_ids: Vec<String> = agent1_tasks.iter().map(|t| t.id.clone()).collect();
    assert!(agent1_ids.contains(&task1.id));
    assert!(agent1_ids.contains(&task3.id));

    // Combined filter: owner + status
    let combined_filter = TaskFilter {
        status: Some(TaskStatus::Pending),
        owner: Some("agent1".into()),
        blocked_by: None,
    };
    let combined = store.list(Some(combined_filter)).unwrap();
    assert_eq!(combined.len(), 1);
    assert_eq!(combined[0].id, task1.id);
}

#[test]
fn test_update_fields() {
    let store = TaskStore::in_memory().unwrap();

    let task = store
        .create(TaskCreate {
            subject: "Original subject".to_string(),
            description: "Original description".to_string(),
            active_form: Some("Original".into()),
            owner: Some("owner1".into()),
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();

    // Update subject
    let updated = store
        .update(
            &task.id,
            TaskUpdate {
                subject: Some("New subject".into()),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(updated.subject, "New subject");
    assert_eq!(updated.description, "Original description");

    // Update multiple fields
    let mut metadata = HashMap::new();
    metadata.insert("key1".to_string(), serde_json::json!("value1"));

    let updated = store
        .update(
            &task.id,
            TaskUpdate {
                description: Some("New description".into()),
                active_form: Some(Some("Updated".into())),
                status: Some(TaskStatus::InProgress),
                owner: Some(Some("owner2".into())),
                metadata: Some(metadata.clone()),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();

    assert_eq!(updated.description, "New description");
    assert_eq!(updated.active_form, Some("Updated".into()));
    assert_eq!(updated.status, TaskStatus::InProgress);
    assert_eq!(updated.owner, Some("owner2".into()));
    assert_eq!(updated.metadata.get("key1").unwrap(), "value1");
}

#[test]
fn test_delete_task() {
    let store = TaskStore::in_memory().unwrap();

    let task = store
        .create(TaskCreate {
            subject: "To delete".to_string(),
            description: "Will be deleted".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();

    // Verify it exists
    assert!(store.get(&task.id).unwrap().is_some());

    // Delete it
    assert!(store.delete(&task.id).unwrap());

    // Verify it's gone
    assert!(store.get(&task.id).unwrap().is_none());

    // Delete again should return false
    assert!(!store.delete(&task.id).unwrap());
}

#[test]
fn test_nonexistent_task_operations() {
    let store = TaskStore::in_memory().unwrap();

    let fake_id = "nonexistent-task-id";

    // Get should return None
    assert!(store.get(fake_id).unwrap().is_none());

    // Update should return None
    let result = store
        .update(
            fake_id,
            TaskUpdate {
                subject: Some("New".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(result.is_none());

    // Set status should return None
    let result = store.set_status(fake_id, TaskStatus::Completed).unwrap();
    assert!(result.is_none());

    // Delete should return false
    assert!(!store.delete(fake_id).unwrap());
}

#[test]
fn test_create_with_dependencies() {
    let store = TaskStore::in_memory().unwrap();

    let task1 = store
        .create(TaskCreate {
            subject: "Task 1".to_string(),
            description: "First".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();

    // Create task2 that depends on task1
    let task2 = store
        .create(TaskCreate {
            subject: "Task 2".to_string(),
            description: "Second".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![task1.id.clone()],
            metadata: HashMap::new(),
        })
        .unwrap();

    assert!(task2.blocked_by.contains(&task1.id));

    // Verify task1 blocks task2
    let task1_updated = store.get(&task1.id).unwrap().unwrap();
    assert!(task1_updated.blocks.contains(&task2.id));
}

#[test]
fn test_create_with_invalid_dependencies() {
    let store = TaskStore::in_memory().unwrap();

    // Try to create task with non-existent dependency
    let result = store.create(TaskCreate {
        subject: "Task 1".to_string(),
        description: "Description".to_string(),
        active_form: None,
        owner: None,
        blocks: vec![],
        blocked_by: vec!["fake-id".into()],
        metadata: HashMap::new(),
    });

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn test_active_form_optional() {
    let store = TaskStore::in_memory().unwrap();

    // Create with active_form
    let task1 = store
        .create(TaskCreate {
            subject: "Task 1".to_string(),
            description: "Description".to_string(),
            active_form: Some("Working".into()),
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();
    assert_eq!(task1.active_form, Some("Working".into()));

    // Create without active_form
    let task2 = store
        .create(TaskCreate {
            subject: "Task 2".to_string(),
            description: "Description".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();
    assert_eq!(task2.active_form, None);
}

#[test]
fn test_metadata_operations() {
    let store = TaskStore::in_memory().unwrap();

    let mut metadata = HashMap::new();
    metadata.insert("priority".to_string(), serde_json::json!("high"));
    metadata.insert("story_points".to_string(), serde_json::json!(5));
    metadata.insert("tags".to_string(), serde_json::json!(["backend", "api"]));

    let task = store
        .create(TaskCreate {
            subject: "Task with metadata".to_string(),
            description: "Description".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: metadata.clone(),
        })
        .unwrap();

    assert_eq!(task.metadata.get("priority").unwrap(), "high");
    assert_eq!(task.metadata.get("story_points").unwrap(), 5);
    assert_eq!(
        task.metadata.get("tags").unwrap(),
        &serde_json::json!(["backend", "api"])
    );

    // Update metadata
    let mut new_metadata = HashMap::new();
    new_metadata.insert("priority".to_string(), serde_json::json!("urgent"));
    new_metadata.insert("assigned".to_string(), serde_json::json!("alice"));

    let updated = store
        .update(
            &task.id,
            TaskUpdate {
                metadata: Some(new_metadata),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();

    assert_eq!(updated.metadata.get("priority").unwrap(), "urgent");
    assert_eq!(updated.metadata.get("assigned").unwrap(), "alice");
    // Old metadata should be replaced
    assert!(!updated.metadata.contains_key("story_points"));
}

#[test]
fn test_list_ordering() {
    let store = TaskStore::in_memory().unwrap();

    // Create tasks in sequence
    let _task1 = store
        .create(TaskCreate {
            subject: "Task 1".to_string(),
            description: "First".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();

    // Small delay to ensure different timestamps
    std::thread::sleep(std::time::Duration::from_millis(1100));

    let _task2 = store
        .create(TaskCreate {
            subject: "Task 2".to_string(),
            description: "Second".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(1100));

    let _task3 = store
        .create(TaskCreate {
            subject: "Task 3".to_string(),
            description: "Third".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();

    // List should be ordered by created_at DESC (most recent first)
    let tasks = store.list(None).unwrap();
    assert_eq!(tasks.len(), 3);
    assert_eq!(tasks[0].subject, "Task 3");
    assert_eq!(tasks[1].subject, "Task 2");
    assert_eq!(tasks[2].subject, "Task 1");
}
