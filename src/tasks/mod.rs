//! Task management system with dependency tracking and cycle detection.
//!
//! This module provides a task management system similar to Claude Code's TaskList/TaskCreate/TaskUpdate tools,
//! with support for:
//! - CRUD operations on tasks
//! - Dependency tracking (blocks/blockedBy)
//! - Cycle detection in dependency graphs
//! - Persistent SQLite storage
//! - Status workflow: pending → in_progress → completed

pub mod store;

#[cfg(test)]
mod tasks_test;

pub use store::{Task, TaskCreate, TaskFilter, TaskStatus, TaskStore, TaskUpdate};
