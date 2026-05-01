//! SQLite-backed task store with dependency tracking.

use crate::config::data_dir;
use crate::error::{FerroError, Result};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Task status following the workflow: pending → in_progress → completed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
}

impl TaskStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            _ => Err("invalid task status"),
        }
    }
}

/// A task with dependencies and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_form: Option<String>,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    pub blocks: Vec<String>,
    pub blocked_by: Vec<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
}

/// Filter options for listing tasks
#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub status: Option<TaskStatus>,
    pub owner: Option<String>,
    pub blocked_by: Option<String>,
}

pub struct TaskStore {
    conn: Connection,
}

#[derive(Debug, Default, Clone)]
pub struct TaskCreate {
    pub subject: String,
    pub description: String,
    pub active_form: Option<String>,
    pub owner: Option<String>,
    pub blocks: Vec<String>,
    pub blocked_by: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Default, Clone)]
pub struct TaskUpdate {
    pub subject: Option<String>,
    pub description: Option<String>,
    pub active_form: Option<Option<String>>,
    pub status: Option<TaskStatus>,
    pub owner: Option<Option<String>>,
    pub blocks: Option<Vec<String>>,
    pub blocked_by: Option<Vec<String>>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl TaskStore {
    pub fn new(db_path: Option<PathBuf>) -> Result<Self> {
        let path = db_path.unwrap_or_else(|| data_dir().join("tasks.db"));

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&path)
            .map_err(|e| FerroError::Memory(format!("Failed to open task database: {e}")))?;

        let store = Self { conn };
        store.initialize_tables()?;
        Ok(store)
    }

    /// Create an in-memory store for testing.
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| FerroError::Memory(format!("Failed to create in-memory db: {e}")))?;
        let store = Self { conn };
        store.initialize_tables()?;
        Ok(store)
    }

    fn initialize_tables(&self) -> Result<()> {
        self.conn
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS tasks (
                    id TEXT PRIMARY KEY,
                    subject TEXT NOT NULL,
                    description TEXT NOT NULL,
                    active_form TEXT,
                    status TEXT NOT NULL DEFAULT 'pending',
                    owner TEXT,
                    blocks TEXT DEFAULT '[]',
                    blocked_by TEXT DEFAULT '[]',
                    metadata TEXT DEFAULT '{}',
                    created_at TEXT NOT NULL DEFAULT (datetime('now')),
                    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
                CREATE INDEX IF NOT EXISTS idx_tasks_owner ON tasks(owner);
                ",
            )
            .map_err(|e| FerroError::Memory(format!("Failed to initialize tables: {e}")))?;
        Ok(())
    }

    /// Generate a unique task ID
    fn generate_id(&self) -> Result<String> {
        // Use timestamp + random component for uniqueness
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| FerroError::Memory(format!("Time error: {e}")))?;
        let random: u32 = rand::random();
        Ok(format!("{}-{:08x}", timestamp.as_secs(), random))
    }

    /// Create a new task
    pub fn create(&self, input: TaskCreate) -> Result<Task> {
        let TaskCreate {
            subject,
            description,
            active_form,
            owner,
            blocks,
            blocked_by,
            metadata,
        } = input;
        let id = self.generate_id()?;

        // Validate that blocked tasks exist
        for task_id in &blocked_by {
            if self.get_raw(task_id)?.is_none() {
                return Err(FerroError::Memory(format!(
                    "Blocked dependency not found: {task_id}"
                )));
            }
        }

        // Check for cycles
        self.check_cycles(&id, &blocks, &blocked_by)?;

        let blocks_json = serde_json::to_string(&blocks)
            .map_err(|e| FerroError::Memory(format!("Failed to serialize blocks: {e}")))?;
        let blocked_by_json = serde_json::to_string(&blocked_by)
            .map_err(|e| FerroError::Memory(format!("Failed to serialize blocked_by: {e}")))?;
        let metadata_json = serde_json::to_string(&metadata)
            .map_err(|e| FerroError::Memory(format!("Failed to serialize metadata: {e}")))?;

        self.conn
            .execute(
                "INSERT INTO tasks (id, subject, description, active_form, status, owner, blocks, blocked_by, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id,
                    subject,
                    description,
                    active_form,
                    TaskStatus::Pending.as_str(),
                    owner,
                    blocks_json,
                    blocked_by_json,
                    metadata_json,
                ],
            )
            .map_err(|e| FerroError::Memory(format!("Failed to create task: {e}")))?;

        // Update reverse dependencies
        for blocked_id in &blocks {
            self.add_blocked_by(blocked_id, &id)?;
        }
        // Also update reverse for blocked_by (if task2 is blocked by task1, task1 should block task2)
        for blocking_id in &blocked_by {
            self.add_block_to_task(blocking_id, &id)?;
        }

        self.get(&id)?
            .ok_or_else(|| FerroError::Memory("Task not found after creation".into()))
    }

    /// Add a task to the blocks list of another task (internal helper, opposite of add_blocked_by)
    fn add_block_to_task(&self, task_id: &str, block_id: &str) -> Result<()> {
        let task = self
            .get_raw(task_id)?
            .ok_or_else(|| FerroError::Memory(format!("Task not found: {task_id}")))?;

        let mut blocks = task.blocks.clone();

        if !blocks.contains(&block_id.to_string()) {
            blocks.push(block_id.to_string());
            let blocks_json = serde_json::to_string(&blocks)
                .map_err(|e| FerroError::Memory(format!("Failed to serialize blocks: {e}")))?;

            self.conn
                .execute(
                    "UPDATE tasks SET blocks = ?1, updated_at = datetime('now') WHERE id = ?2",
                    params![blocks_json, task_id],
                )
                .map_err(|e| FerroError::Memory(format!("Failed to add block: {e}")))?;
        }

        Ok(())
    }

    /// Add a task to the blocked_by list of another task (internal helper)
    fn add_blocked_by(&self, task_id: &str, blocked_by_id: &str) -> Result<()> {
        let task = self
            .get_raw(task_id)?
            .ok_or_else(|| FerroError::Memory(format!("Task not found: {task_id}")))?;

        let mut blocked_by = task.blocked_by.clone();

        if !blocked_by.contains(&blocked_by_id.to_string()) {
            blocked_by.push(blocked_by_id.to_string());
            let blocked_by_json = serde_json::to_string(&blocked_by)
                .map_err(|e| FerroError::Memory(format!("Failed to serialize blocked_by: {e}")))?;

            self.conn
                .execute(
                    "UPDATE tasks SET blocked_by = ?1, updated_at = datetime('now') WHERE id = ?2",
                    params![blocked_by_json, task_id],
                )
                .map_err(|e| FerroError::Memory(format!("Failed to update blocked_by: {e}")))?;
        }

        Ok(())
    }

    /// Check for cycles in the dependency graph
    fn check_cycles(&self, id: &str, blocks: &[String], blocked_by: &[String]) -> Result<()> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        // Check if adding these dependencies creates a cycle
        for blocked_id in blocked_by {
            if self.has_cycle_from(blocked_id, id, &mut visited, &mut rec_stack)? {
                return Err(FerroError::Memory(format!(
                    "Cycle detected: {id} is blocked by {blocked_id}, but a dependency path exists from {blocked_id} to {id}"
                )));
            }
        }

        for block_id in blocks {
            if self.has_cycle_from(id, block_id, &mut visited, &mut rec_stack)? {
                return Err(FerroError::Memory(format!(
                    "Cycle detected: {id} blocks {block_id}, but a dependency path exists from {block_id} to {id}"
                )));
            }
        }

        Ok(())
    }

    /// DFS-based cycle detection
    fn has_cycle_from(
        &self,
        start: &str,
        target: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
    ) -> Result<bool> {
        if start == target {
            return Ok(true);
        }

        if visited.contains(start) {
            return Ok(rec_stack.contains(start));
        }

        visited.insert(start.to_string());
        rec_stack.insert(start.to_string());

        if let Some(task) = self.get_raw(start)? {
            let blocked_by = task.blocked_by.clone();

            for dep in &blocked_by {
                if self.has_cycle_from(dep, target, visited, rec_stack)? {
                    return Ok(true);
                }
            }
        }

        rec_stack.remove(start);
        Ok(false)
    }

    /// Get a task by ID with full deserialization
    pub fn get(&self, id: &str) -> Result<Option<Task>> {
        self.get_raw(id)
    }

    /// Internal helper to get and deserialize a task
    fn get_raw(&self, id: &str) -> Result<Option<Task>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM tasks WHERE id = ?1")
            .map_err(|e| FerroError::Memory(format!("Get prepare failed: {e}")))?;

        let result = stmt.query_row(params![id], |row| {
            let blocks_str: String = row.get(6)?;
            let blocked_by_str: String = row.get(7)?;
            let metadata_str: String = row.get(8)?;

            Ok(Task {
                id: row.get(0)?,
                subject: row.get(1)?,
                description: row.get(2)?,
                active_form: row.get(3)?,
                status: row
                    .get::<_, String>(4)?
                    .parse::<TaskStatus>()
                    .unwrap_or(TaskStatus::Pending),
                owner: row.get(5)?,
                blocks: serde_json::from_str(&blocks_str).unwrap_or_default(),
                blocked_by: serde_json::from_str(&blocked_by_str).unwrap_or_default(),
                metadata: serde_json::from_str(&metadata_str).unwrap_or_default(),
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        });

        match result {
            Ok(task) => Ok(Some(task)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(FerroError::Memory(format!("Get task failed: {e}"))),
        }
    }

    /// List tasks with optional filtering
    pub fn list(&self, filter: Option<TaskFilter>) -> Result<Vec<Task>> {
        let mut query = "SELECT * FROM tasks".to_string();
        let mut conditions = Vec::new();
        let mut params_vec: Vec<String> = Vec::new();

        if let Some(f) = filter {
            if let Some(status) = f.status {
                conditions.push("status = ?".to_string());
                params_vec.push(status.as_str().to_string());
            }
            if let Some(owner) = f.owner {
                conditions.push("owner = ?".to_string());
                params_vec.push(owner);
            }
            if let Some(blocked_by) = f.blocked_by {
                conditions.push("blocked_by LIKE ?".to_string());
                params_vec.push(format!("%{blocked_by}%"));
            }
        }

        if !conditions.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&conditions.join(" AND "));
        }

        query.push_str(" ORDER BY created_at DESC");

        let mut stmt = self
            .conn
            .prepare(&query)
            .map_err(|e| FerroError::Memory(format!("List prepare failed: {e}")))?;

        let params: Vec<&dyn rusqlite::ToSql> = params_vec
            .iter()
            .map(|s| s as &dyn rusqlite::ToSql)
            .collect();

        let tasks = stmt
            .query_map(&params[..], |row| {
                let blocks_str: String = row.get(6)?;
                let blocked_by_str: String = row.get(7)?;
                let metadata_str: String = row.get(8)?;

                Ok(Task {
                    id: row.get(0)?,
                    subject: row.get(1)?,
                    description: row.get(2)?,
                    active_form: row.get(3)?,
                    status: row
                        .get::<_, String>(4)?
                        .parse::<TaskStatus>()
                        .unwrap_or(TaskStatus::Pending),
                    owner: row.get(5)?,
                    blocks: serde_json::from_str(&blocks_str).unwrap_or_default(),
                    blocked_by: serde_json::from_str(&blocked_by_str).unwrap_or_default(),
                    metadata: serde_json::from_str(&metadata_str).unwrap_or_default(),
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            })
            .map_err(|e| FerroError::Memory(format!("List query failed: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tasks)
    }

    /// Update task fields
    pub fn update(&self, id: &str, update: TaskUpdate) -> Result<Option<Task>> {
        let TaskUpdate {
            subject,
            description,
            active_form,
            status,
            owner,
            blocks,
            blocked_by,
            metadata,
        } = update;
        // Check task exists
        let existing = self.get_raw(id)?;
        if existing.is_none() {
            return Ok(None);
        }

        // Check for cycles if dependencies are being updated
        if blocks.is_some() || blocked_by.is_some() {
            let empty = Vec::new();
            let new_blocks = blocks.as_ref().unwrap_or(&empty);
            let new_blocked_by = blocked_by.as_ref().unwrap_or(&empty);
            self.check_cycles(id, new_blocks, new_blocked_by)?;
        }

        let mut set_clauses = Vec::new();
        let mut params_vec = Vec::new();

        if let Some(s) = subject {
            set_clauses.push("subject = ?");
            params_vec.push(s);
        }
        if let Some(d) = description {
            set_clauses.push("description = ?");
            params_vec.push(d);
        }
        if let Some(af) = active_form {
            set_clauses.push("active_form = ?");
            params_vec.push(af.map(|v| v as String).unwrap_or_else(String::new));
        }
        if let Some(s) = status {
            set_clauses.push("status = ?");
            params_vec.push(s.as_str().to_string());
        }
        if let Some(o) = owner {
            set_clauses.push("owner = ?");
            params_vec.push(o.map(|v| v as String).unwrap_or_else(String::new));
        }
        if let Some(b) = blocks {
            let b_json = serde_json::to_string(&b)
                .map_err(|e| FerroError::Memory(format!("Failed to serialize blocks: {e}")))?;
            set_clauses.push("blocks = ?");
            params_vec.push(b_json);
        }
        if let Some(bb) = blocked_by {
            let bb_json = serde_json::to_string(&bb)
                .map_err(|e| FerroError::Memory(format!("Failed to serialize blocked_by: {e}")))?;
            set_clauses.push("blocked_by = ?");
            params_vec.push(bb_json);
        }
        if let Some(m) = metadata {
            let m_json = serde_json::to_string(&m)
                .map_err(|e| FerroError::Memory(format!("Failed to serialize metadata: {e}")))?;
            set_clauses.push("metadata = ?");
            params_vec.push(m_json);
        }

        if set_clauses.is_empty() {
            return self.get(id);
        }

        set_clauses.push("updated_at = datetime('now')");

        let query = format!("UPDATE tasks SET {} WHERE id = ?", set_clauses.join(", "));
        params_vec.push(id.to_string());

        let params: Vec<&dyn rusqlite::ToSql> = params_vec
            .iter()
            .map(|s| s as &dyn rusqlite::ToSql)
            .collect();

        self.conn
            .execute(&query, &params[..])
            .map_err(|e| FerroError::Memory(format!("Update failed: {e}")))?;

        self.get(id)
    }

    /// Delete a task by ID
    pub fn delete(&self, id: &str) -> Result<bool> {
        let rows = self
            .conn
            .execute("DELETE FROM tasks WHERE id = ?1", params![id])
            .map_err(|e| FerroError::Memory(format!("Delete failed: {e}")))?;

        Ok(rows > 0)
    }

    /// Add a dependency (task blocks another task)
    pub fn add_block(&self, id: &str, blocks_id: &str) -> Result<Option<Task>> {
        let task = self
            .get_raw(id)?
            .ok_or_else(|| FerroError::Memory(format!("Task not found: {id}")))?;

        let mut blocks = task.blocks.clone();

        if blocks.contains(&blocks_id.to_string()) {
            return Ok(Some(task)); // Already exists
        }

        // Check for cycle
        self.check_cycles(id, &[blocks_id.to_string()], &[])?;

        blocks.push(blocks_id.to_string());

        let blocks_json = serde_json::to_string(&blocks)
            .map_err(|e| FerroError::Memory(format!("Failed to serialize blocks: {e}")))?;

        self.conn
            .execute(
                "UPDATE tasks SET blocks = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![blocks_json, id],
            )
            .map_err(|e| FerroError::Memory(format!("Failed to add block: {e}")))?;

        // Update reverse dependency
        self.add_blocked_by(blocks_id, id)?;

        self.get(id)
    }

    /// Remove a dependency
    pub fn remove_block(&self, id: &str, blocks_id: &str) -> Result<Option<Task>> {
        let task = self
            .get_raw(id)?
            .ok_or_else(|| FerroError::Memory(format!("Task not found: {id}")))?;

        let mut blocks = task.blocks.clone();

        if !blocks.contains(&blocks_id.to_string()) {
            return Ok(Some(task)); // Doesn't exist
        }

        blocks.retain(|x| x != blocks_id);

        let blocks_json = serde_json::to_string(&blocks)
            .map_err(|e| FerroError::Memory(format!("Failed to serialize blocks: {e}")))?;

        self.conn
            .execute(
                "UPDATE tasks SET blocks = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![blocks_json, id],
            )
            .map_err(|e| FerroError::Memory(format!("Failed to remove block: {e}")))?;

        // Remove reverse dependency
        self.remove_blocked_by(blocks_id, id)?;

        self.get(id)
    }

    /// Remove from blocked_by list (internal helper)
    fn remove_blocked_by(&self, task_id: &str, blocked_by_id: &str) -> Result<()> {
        let task = self
            .get_raw(task_id)?
            .ok_or_else(|| FerroError::Memory(format!("Task not found: {task_id}")))?;

        let mut blocked_by = task.blocked_by.clone();
        blocked_by.retain(|x| x != blocked_by_id);

        let blocked_by_json = serde_json::to_string(&blocked_by)
            .map_err(|e| FerroError::Memory(format!("Failed to serialize blocked_by: {e}")))?;

        self.conn
            .execute(
                "UPDATE tasks SET blocked_by = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![blocked_by_json, task_id],
            )
            .map_err(|e| FerroError::Memory(format!("Failed to update blocked_by: {e}")))?;

        Ok(())
    }

    /// Set task status
    pub fn set_status(&self, id: &str, status: TaskStatus) -> Result<Option<Task>> {
        self.conn
            .execute(
                "UPDATE tasks SET status = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![status.as_str(), id],
            )
            .map_err(|e| FerroError::Memory(format!("Failed to set status: {e}")))?;

        self.get(id)
    }

    /// Get tasks that are blocking a given task (tasks it depends on)
    pub fn get_blocking(&self, id: &str) -> Result<Vec<Task>> {
        let task = self
            .get_raw(id)?
            .ok_or_else(|| FerroError::Memory(format!("Task not found: {id}")))?;

        let mut result = Vec::new();
        for task_id in &task.blocked_by {
            if let Some(t) = self.get(task_id)? {
                result.push(t);
            }
        }

        Ok(result)
    }

    /// Get tasks that a given task is blocking (tasks that depend on it)
    pub fn get_blocked(&self, id: &str) -> Result<Vec<Task>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM tasks WHERE blocked_by LIKE ?")
            .map_err(|e| FerroError::Memory(format!("Get blocked prepare failed: {e}")))?;

        let pattern = format!("%{id}%");
        eprintln!("DEBUG get_blocked({}): query pattern = {}", id, pattern);

        let tasks = stmt
            .query_map(params![pattern], |row| {
                let blocks_str: String = row.get(6)?;
                let blocked_by_str: String = row.get(7)?;
                let metadata_str: String = row.get(8)?;

                Ok(Task {
                    id: row.get(0)?,
                    subject: row.get(1)?,
                    description: row.get(2)?,
                    active_form: row.get(3)?,
                    status: row
                        .get::<_, String>(4)?
                        .parse::<TaskStatus>()
                        .unwrap_or(TaskStatus::Pending),
                    owner: row.get(5)?,
                    blocks: serde_json::from_str(&blocks_str).unwrap_or_default(),
                    blocked_by: serde_json::from_str(&blocked_by_str).unwrap_or_default(),
                    metadata: serde_json::from_str(&metadata_str).unwrap_or_default(),
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            })
            .map_err(|e| FerroError::Memory(format!("Get blocked query failed: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tasks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_crud() {
        let store = TaskStore::in_memory().unwrap();

        // Create
        let task = store
            .create(TaskCreate {
                subject: "Test task".to_string(),
                description: "Test description".to_string(),
                active_form: Some("Testing".into()),
                owner: None,
                blocks: vec![],
                blocked_by: vec![],
                metadata: HashMap::new(),
            })
            .unwrap();

        assert_eq!(task.subject, "Test task");
        assert_eq!(task.status, TaskStatus::Pending);

        // Get
        let retrieved = store.get(&task.id).unwrap().unwrap();
        assert_eq!(retrieved.id, task.id);
        assert_eq!(retrieved.subject, "Test task");

        // Update status
        let updated = store
            .set_status(&task.id, TaskStatus::InProgress)
            .unwrap()
            .unwrap();
        assert_eq!(updated.status, TaskStatus::InProgress);

        // Delete
        assert!(store.delete(&task.id).unwrap());
        assert!(store.get(&task.id).unwrap().is_none());
    }

    #[test]
    fn test_task_dependencies() {
        let store = TaskStore::in_memory().unwrap();

        let _task1 = store
            .create(TaskCreate {
                subject: "Task 1".to_string(),
                description: "First task".to_string(),
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
                description: "Second task".to_string(),
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
                description: "Third task".to_string(),
                active_form: None,
                owner: None,
                blocks: vec![],
                blocked_by: vec![],
                metadata: HashMap::new(),
            })
            .unwrap();

        // task2 blocks task3 (task3 depends on task2)
        let task2_updated = store.add_block(&task2.id, &task3.id).unwrap().unwrap();
        assert!(task2_updated.blocks.contains(&task3.id));

        let task3_retrieved = store.get(&task3.id).unwrap().unwrap();
        assert!(task3_retrieved.blocked_by.contains(&task2.id));

        // Get blocking tasks for task3
        let blocking = store.get_blocking(&task3.id).unwrap();
        assert_eq!(blocking.len(), 1);
        assert_eq!(blocking[0].id, task2.id);

        // Get blocked tasks for task2
        let blocked = store.get_blocked(&task2.id).unwrap();
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].id, task3.id);

        // Remove dependency
        store.remove_block(&task2.id, &task3.id).unwrap().unwrap();
        let task2_final = store.get(&task2.id).unwrap().unwrap();
        assert!(!task2_final.blocks.contains(&task3.id));
    }

    #[test]
    fn test_cycle_detection() {
        let store = TaskStore::in_memory().unwrap();

        let task1 = store
            .create(TaskCreate {
                subject: "Task 1".to_string(),
                description: "First task".to_string(),
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
                description: "Second task".to_string(),
                active_form: None,
                owner: None,
                blocks: vec![],
                blocked_by: vec![],
                metadata: HashMap::new(),
            })
            .unwrap();

        // Create dependency: task1 blocks task2
        store.add_block(&task1.id, &task2.id).unwrap();

        // Try to create reverse dependency (should fail - cycle)
        let result = store.add_block(&task2.id, &task1.id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cycle detected"));
    }

    #[test]
    fn test_complex_cycle_detection() {
        let store = TaskStore::in_memory().unwrap();

        let task1 = store
            .create(TaskCreate {
                subject: "Task 1".to_string(),
                description: "First task".to_string(),
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
                description: "Second task".to_string(),
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
                description: "Third task".to_string(),
                active_form: None,
                owner: None,
                blocks: vec![],
                blocked_by: vec![],
                metadata: HashMap::new(),
            })
            .unwrap();

        // Create chain: task1 -> task2 -> task3
        store.add_block(&task1.id, &task2.id).unwrap();
        store.add_block(&task2.id, &task3.id).unwrap();

        // Try to create cycle: task3 -> task1 (should fail)
        let result = store.add_block(&task3.id, &task1.id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cycle detected"));
    }

    #[test]
    fn test_list_with_filters() {
        let store = TaskStore::in_memory().unwrap();

        store
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
        store
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

        // Update task2 status
        let task2 = store.list(None).unwrap()[1].clone();
        store.set_status(&task2.id, TaskStatus::Completed).unwrap();

        // Filter by status
        let pending_filter = TaskFilter {
            status: Some(TaskStatus::Pending),
            owner: None,
            blocked_by: None,
        };
        let pending = store.list(Some(pending_filter)).unwrap();
        assert_eq!(pending.len(), 1);

        // Filter by owner
        let owner_filter = TaskFilter {
            status: None,
            owner: Some("agent1".into()),
            blocked_by: None,
        };
        let agent1_tasks = store.list(Some(owner_filter)).unwrap();
        assert_eq!(agent1_tasks.len(), 1);
        assert_eq!(agent1_tasks[0].owner, Some("agent1".into()));
    }

    #[test]
    fn test_update_with_metadata() {
        let store = TaskStore::in_memory().unwrap();

        let task = store
            .create(TaskCreate {
                subject: "Task 1".to_string(),
                description: "Description".to_string(),
                active_form: None,
                owner: None,
                blocks: vec![],
                blocked_by: vec![],
                metadata: HashMap::new(),
            })
            .unwrap();

        let mut metadata = HashMap::new();
        metadata.insert("priority".to_string(), serde_json::json!("high"));
        metadata.insert("estimated_hours".to_string(), serde_json::json!(5));

        let updated = store
            .update(
                &task.id,
                TaskUpdate {
                    metadata: Some(metadata),
                    ..Default::default()
                },
            )
            .unwrap()
            .unwrap();

        assert_eq!(updated.metadata.get("priority").unwrap(), "high");
        assert_eq!(updated.metadata.get("estimated_hours").unwrap(), 5);
    }

    #[test]
    fn test_nonexistent_dependency() {
        let store = TaskStore::in_memory().unwrap();

        // Try to create task with non-existent dependency
        let result = store.create(TaskCreate {
            subject: "Task 1".to_string(),
            description: "Description".to_string(),
            active_form: None,
            owner: None,
            blocks: vec![],
            blocked_by: vec!["nonexistent".into()],
            metadata: HashMap::new(),
        });

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
