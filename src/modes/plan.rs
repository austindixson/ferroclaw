//! Plan Mode: Structured multi-phase planning system for Ferroclaw.
//!
//! PlanMode provides a state machine for managing complex, multi-step tasks:
//! - Research: Gather information and understand requirements
//! - Planning: Create detailed steps with dependencies and acceptance criteria
//! - Implementation: Execute steps in waves based on dependencies
//! - Verification: Validate outcomes against acceptance criteria
//!
//! The system uses TaskSystem for persistent storage and dependency tracking.

use crate::error::{FerroError, Result};
use crate::tasks::{Task, TaskCreate, TaskStatus, TaskStore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Planning phases in order
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanPhase {
    /// Gather information and understand requirements
    Research,
    /// Create detailed steps with dependencies
    Planning,
    /// Execute steps in dependency-based waves
    Implementation,
    /// Validate outcomes against acceptance criteria
    Verification,
}

impl PlanPhase {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Research => "research",
            Self::Planning => "planning",
            Self::Implementation => "implementation",
            Self::Verification => "verification",
        }
    }

    /// Get the next phase in the sequence
    pub fn next(&self) -> Option<Self> {
        match self {
            Self::Research => Some(Self::Planning),
            Self::Planning => Some(Self::Implementation),
            Self::Implementation => Some(Self::Verification),
            Self::Verification => None, // Terminal phase
        }
    }
}

/// A single step in a plan with acceptance criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// Unique step identifier (matches Task ID)
    pub id: String,
    /// Brief title for the step
    pub subject: String,
    /// Detailed description of what needs to be done
    pub description: String,
    /// Present continuous form shown in progress indicators
    pub active_form: Option<String>,
    /// Acceptance criteria for verification
    pub acceptance_criteria: Vec<String>,
    /// IDs of steps this step depends on
    pub depends_on: Vec<String>,
    /// IDs of steps that depend on this step
    pub blocks: Vec<String>,
    /// Current status
    pub status: PlanStepStatus,
    /// Assigned wave number (0 = can start immediately)
    pub wave: usize,
    /// Whether this step requires approval before proceeding
    pub requires_approval: bool,
    /// Whether approval has been granted
    pub approval_granted: bool,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
    /// Creation timestamp
    pub created_at: String,
    /// Last update timestamp
    pub updated_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct CreateStepInput {
    pub subject: String,
    pub description: String,
    pub active_form: Option<String>,
    pub acceptance_criteria: Vec<String>,
    pub depends_on: Vec<String>,
    pub requires_approval: bool,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Status of a plan step
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanStepStatus {
    /// Step is pending (not yet started)
    Pending,
    /// Step is currently being worked on
    InProgress,
    /// Step is completed
    Completed,
    /// Step is blocked by dependencies
    Blocked,
    /// Step is awaiting approval
    AwaitingApproval,
    /// Step failed and needs attention
    Failed,
}

impl PlanStepStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
            Self::AwaitingApproval => "awaiting_approval",
            Self::Failed => "failed",
        }
    }
}

impl std::str::FromStr for PlanPhase {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "research" => Ok(Self::Research),
            "planning" => Ok(Self::Planning),
            "implementation" => Ok(Self::Implementation),
            "verification" => Ok(Self::Verification),
            _ => Err("invalid plan phase"),
        }
    }
}

impl std::str::FromStr for PlanStepStatus {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "blocked" => Ok(Self::Blocked),
            "awaiting_approval" => Ok(Self::AwaitingApproval),
            "failed" => Ok(Self::Failed),
            _ => Err("invalid plan step status"),
        }
    }
}

impl From<TaskStatus> for PlanStepStatus {
    fn from(status: TaskStatus) -> Self {
        match status {
            TaskStatus::Pending => Self::Pending,
            TaskStatus::InProgress => Self::InProgress,
            TaskStatus::Completed => Self::Completed,
        }
    }
}

impl From<PlanStepStatus> for TaskStatus {
    fn from(status: PlanStepStatus) -> Self {
        match status {
            PlanStepStatus::Pending
            | PlanStepStatus::Blocked
            | PlanStepStatus::AwaitingApproval
            | PlanStepStatus::Failed => Self::Pending,
            PlanStepStatus::InProgress => Self::InProgress,
            PlanStepStatus::Completed => Self::Completed,
        }
    }
}

/// Approval gate that blocks phase transitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalGate {
    /// Phase this gate guards
    pub phase: PlanPhase,
    /// Whether approval has been granted
    pub approved: bool,
    /// Approval notes or justification
    pub notes: Option<String>,
    /// Timestamp of approval
    pub approved_at: Option<String>,
}

/// Wave of steps that can be executed in parallel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wave {
    /// Wave number (0-indexed)
    pub number: usize,
    /// Step IDs in this wave
    pub step_ids: Vec<String>,
    /// Whether this wave has been completed
    pub completed: bool,
}

/// Plan mode state machine
pub struct PlanMode {
    /// Current planning phase
    phase: PlanPhase,
    /// Task store for persistent step storage
    store: TaskStore,
    /// Approval gates for phase transitions
    approval_gates: HashMap<PlanPhase, ApprovalGate>,
    /// Cached steps (lazily loaded from store)
    steps_cache: HashMap<String, PlanStep>,
    /// Whether cache is dirty and needs reload
    cache_dirty: bool,
}

impl PlanMode {
    /// Create a new PlanMode instance
    pub fn new(db_path: Option<PathBuf>) -> Result<Self> {
        let store = TaskStore::new(db_path)?;

        let mut approval_gates = HashMap::new();
        // Initialize gates for all phases
        approval_gates.insert(
            PlanPhase::Research,
            ApprovalGate {
                phase: PlanPhase::Research,
                approved: false, // Requires approval to exit
                notes: None,
                approved_at: None,
            },
        );
        approval_gates.insert(
            PlanPhase::Planning,
            ApprovalGate {
                phase: PlanPhase::Planning,
                approved: false,
                notes: None,
                approved_at: None,
            },
        );
        approval_gates.insert(
            PlanPhase::Implementation,
            ApprovalGate {
                phase: PlanPhase::Implementation,
                approved: false,
                notes: None,
                approved_at: None,
            },
        );
        approval_gates.insert(
            PlanPhase::Verification,
            ApprovalGate {
                phase: PlanPhase::Verification,
                approved: false,
                notes: None,
                approved_at: None,
            },
        );

        Ok(Self {
            phase: PlanPhase::Research,
            store,
            approval_gates,
            steps_cache: HashMap::new(),
            cache_dirty: true,
        })
    }

    /// Get the current phase
    pub fn phase(&self) -> PlanPhase {
        self.phase
    }

    /// Transition to the next phase (requires approval if gate exists)
    pub fn transition_phase(&mut self, _notes: Option<String>) -> Result<PlanPhase> {
        // Check if current phase requires approval to exit
        if let Some(gate) = self.approval_gates.get(&self.phase)
            && !gate.approved
        {
            return Err(FerroError::Memory(format!(
                "Cannot transition from {}: approval required. Use approve_phase() first.",
                self.phase.as_str()
            )));
        }

        let next = self.phase.next().ok_or_else(|| {
            FerroError::Memory(format!(
                "Cannot transition from {}: already in terminal phase",
                self.phase.as_str()
            ))
        })?;

        self.phase = next;
        Ok(next)
    }

    /// Approve the current phase to allow transition
    pub fn approve_phase(&mut self, notes: Option<String>) -> Result<()> {
        let gate = self.approval_gates.get_mut(&self.phase).ok_or_else(|| {
            FerroError::Memory(format!(
                "No approval gate for phase: {}",
                self.phase.as_str()
            ))
        })?;

        gate.approved = true;
        gate.notes = notes;
        // Use ISO 8601 format timestamp
        gate.approved_at = Some(format!("{:?}", std::time::SystemTime::now()));

        Ok(())
    }

    /// Check if a phase transition is approved
    pub fn is_phase_approved(&self, phase: PlanPhase) -> bool {
        self.approval_gates
            .get(&phase)
            .map(|g| g.approved)
            .unwrap_or(false)
    }

    /// Create a new plan step
    pub fn create_step(&mut self, input: CreateStepInput) -> Result<PlanStep> {
        let CreateStepInput {
            subject,
            description,
            active_form,
            acceptance_criteria,
            depends_on,
            requires_approval,
            metadata,
        } = input;

        // Validate dependencies exist
        for dep_id in &depends_on {
            if self.store.get(dep_id)?.is_none() {
                return Err(FerroError::Memory(format!(
                    "Dependency step not found: {dep_id}"
                )));
            }
        }

        // Create task in store with approval metadata
        let mut task_metadata = metadata.clone();
        task_metadata.insert(
            "requires_approval".to_string(),
            serde_json::json!(requires_approval),
        );
        task_metadata.insert(
            "acceptance_criteria".to_string(),
            serde_json::json!(acceptance_criteria),
        );

        let task = self.store.create(TaskCreate {
            subject: subject.clone(),
            description: description.clone(),
            active_form: active_form.clone(),
            owner: None,
            blocks: vec![],
            blocked_by: depends_on.clone(),
            metadata: task_metadata,
        })?;

        // Set up bidirectional dependency relationships
        // For each dependency, add this task to the dependency's blocks list
        for dep_id in &depends_on {
            self.store.add_block(dep_id, &task.id)?;
        }

        // Reload task from store to get updated blocks/blocked_by
        let task = self.store.get(&task.id)?.unwrap();

        // Calculate wave number based on dependencies
        let wave = self.calculate_wave(&task.id, &depends_on)?;

        // Create plan step
        let step = PlanStep {
            id: task.id.clone(),
            subject: task.subject,
            description: task.description,
            active_form: task.active_form,
            acceptance_criteria,
            depends_on: task.blocked_by,
            blocks: task.blocks,
            status: if requires_approval {
                PlanStepStatus::AwaitingApproval
            } else if !depends_on.is_empty() {
                PlanStepStatus::Blocked
            } else {
                PlanStepStatus::Pending
            },
            wave,
            requires_approval,
            approval_granted: false,
            metadata: task.metadata,
            created_at: task.created_at,
            updated_at: task.updated_at,
        };

        // Cache the step
        self.steps_cache.insert(step.id.clone(), step.clone());
        Ok(step)
    }

    /// Calculate wave number for a step based on its dependencies
    fn calculate_wave(&mut self, _step_id: &str, depends_on: &[String]) -> Result<usize> {
        if depends_on.is_empty() {
            return Ok(0); // No dependencies -> wave 0
        }

        let mut max_dep_wave = 0;
        for dep_id in depends_on {
            // Load from store directly to avoid mutable borrow conflict
            if let Some(task) = self.store.get(dep_id)? {
                // Recursively calculate wave for dependency
                let dep_wave = self.calculate_wave_for_task(&task)?;
                max_dep_wave = max_dep_wave.max(dep_wave + 1);
            }
        }

        Ok(max_dep_wave)
    }

    /// Helper to calculate wave for a task (used internally to avoid borrow issues)
    fn calculate_wave_for_task(&mut self, task: &Task) -> Result<usize> {
        if task.blocked_by.is_empty() {
            return Ok(0);
        }

        let mut max_dep_wave = 0;
        for dep_id in &task.blocked_by {
            if let Some(dep_task) = self.store.get(dep_id)? {
                let dep_wave = self.calculate_wave_for_task(&dep_task)?;
                max_dep_wave = max_dep_wave.max(dep_wave + 1);
            }
        }

        Ok(max_dep_wave)
    }

    /// Get a step by ID
    pub fn get_step(&mut self, id: &str) -> Result<Option<PlanStep>> {
        self.reload_cache_if_needed()?;

        if let Some(step) = self.steps_cache.get(id) {
            return Ok(Some(step.clone()));
        }

        // Load from store
        let task = match self.store.get(id)? {
            Some(t) => t,
            None => return Ok(None),
        };

        // Convert task to plan step
        let wave = self.calculate_wave(id, &task.blocked_by)?;

        // Extract requires_approval from metadata
        let requires_approval = task
            .metadata
            .get("requires_approval")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Extract acceptance_criteria from metadata
        let acceptance_criteria = task
            .metadata
            .get("acceptance_criteria")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // Extract approval_granted from metadata
        let approval_granted = task
            .metadata
            .get("approval_granted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let status = if requires_approval && !approval_granted {
            PlanStepStatus::AwaitingApproval
        } else if task.status == TaskStatus::Pending && !task.blocked_by.is_empty() {
            PlanStepStatus::Blocked
        } else {
            PlanStepStatus::from(task.status)
        };

        let step = PlanStep {
            id: task.id,
            subject: task.subject,
            description: task.description,
            active_form: task.active_form,
            acceptance_criteria,
            depends_on: task.blocked_by,
            blocks: task.blocks,
            status,
            wave,
            requires_approval,
            approval_granted,
            metadata: task.metadata,
            created_at: task.created_at,
            updated_at: task.updated_at,
        };

        self.steps_cache.insert(step.id.clone(), step.clone());
        Ok(Some(step))
    }

    /// List all steps in the plan
    pub fn list_steps(&mut self) -> Result<Vec<PlanStep>> {
        self.reload_cache_if_needed()?;

        let tasks = self.store.list(None)?;
        let mut steps = Vec::new();

        for task in tasks {
            let wave = self.calculate_wave(&task.id, &task.blocked_by)?;

            // Extract metadata fields
            let requires_approval = task
                .metadata
                .get("requires_approval")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let acceptance_criteria = task
                .metadata
                .get("acceptance_criteria")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            let approval_granted = task
                .metadata
                .get("approval_granted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let status = if requires_approval && !approval_granted {
                PlanStepStatus::AwaitingApproval
            } else if task.status == TaskStatus::Pending && !task.blocked_by.is_empty() {
                PlanStepStatus::Blocked
            } else {
                PlanStepStatus::from(task.status)
            };

            let step = PlanStep {
                id: task.id,
                subject: task.subject,
                description: task.description,
                active_form: task.active_form,
                acceptance_criteria,
                depends_on: task.blocked_by,
                blocks: task.blocks,
                status,
                wave,
                requires_approval,
                approval_granted,
                metadata: task.metadata,
                created_at: task.created_at,
                updated_at: task.updated_at,
            };

            steps.push(step);
        }

        Ok(steps)
    }

    /// Update step status
    pub fn update_step_status(
        &mut self,
        id: &str,
        status: PlanStepStatus,
    ) -> Result<Option<PlanStep>> {
        let task_status = TaskStatus::from(status);
        let updated_task = self.store.set_status(id, task_status)?;

        if let Some(updated_task) = updated_task {
            // Update cache
            if let Some(mut step) = self.get_step(id)? {
                step.status = status;
                step.updated_at = updated_task.updated_at.clone();
                self.steps_cache.insert(id.to_string(), step.clone());

                // Update dependent steps
                if status == PlanStepStatus::Completed {
                    self.update_dependent_steps(id)?;
                }

                return Ok(Some(step));
            }
        }

        Ok(None)
    }

    /// Update dependent steps when a dependency is completed
    fn update_dependent_steps(&mut self, completed_id: &str) -> Result<()> {
        // Find tasks that are blocked by the completed task
        // These are tasks that have the completed task in their blocked_by list
        let tasks = self.store.list(None)?;
        let blocking: Vec<_> = tasks
            .into_iter()
            .filter(|t| t.blocked_by.contains(&completed_id.to_string()))
            .collect();

        for task in blocking {
            // Check if all dependencies are completed
            let all_deps_complete = task.blocked_by.iter().all(|dep_id| {
                self.store
                    .get(dep_id)
                    .ok()
                    .flatten()
                    .map(|t| t.status == TaskStatus::Completed)
                    .unwrap_or(false)
            });

            if all_deps_complete {
                // Move from Blocked to Pending
                let _ = self.store.set_status(&task.id, TaskStatus::Pending);

                // Update cache
                if let Some(step) = self.steps_cache.get_mut(&task.id) {
                    step.status = PlanStepStatus::Pending;
                    step.updated_at = format!("{:?}", std::time::SystemTime::now());
                }
            }
        }

        Ok(())
    }

    /// Grant approval to a step
    pub fn approve_step(&mut self, id: &str) -> Result<Option<PlanStep>> {
        if let Some(mut step) = self.get_step(id)? {
            if !step.requires_approval {
                return Err(FerroError::Memory(format!(
                    "Step {id} does not require approval"
                )));
            }

            step.approval_granted = true;
            step.status = PlanStepStatus::Pending; // Move from AwaitingApproval to Pending
            step.updated_at = format!("{:?}", std::time::SystemTime::now());

            // Update in store - persist approval_granted in metadata
            let mut metadata = step.metadata.clone();
            metadata.insert("approval_granted".to_string(), serde_json::json!(true));
            self.store.update(
                id,
                crate::tasks::TaskUpdate {
                    metadata: Some(metadata),
                    ..Default::default()
                },
            )?;

            // Update in store
            self.store.set_status(id, TaskStatus::Pending)?;

            // Update cache
            self.steps_cache.insert(id.to_string(), step.clone());
            Ok(Some(step))
        } else {
            Ok(None)
        }
    }

    /// Calculate waves based on dependencies
    pub fn calculate_waves(&mut self) -> Result<Vec<Wave>> {
        let steps = self.list_steps()?;
        let mut waves_map: HashMap<usize, Vec<String>> = HashMap::new();

        for step in steps {
            waves_map.entry(step.wave).or_default().push(step.id);
        }

        let mut waves: Vec<Wave> = waves_map
            .into_iter()
            .map(|(number, step_ids)| Wave {
                number,
                step_ids,
                completed: false,
            })
            .collect();

        waves.sort_by_key(|w| w.number);
        Ok(waves)
    }

    /// Get current status summary
    pub fn status(&mut self) -> Result<PlanStatus> {
        let steps = self.list_steps()?;
        let waves = self.calculate_waves()?;

        let completed = steps
            .iter()
            .filter(|s| s.status == PlanStepStatus::Completed)
            .count();
        let pending = steps
            .iter()
            .filter(|s| s.status == PlanStepStatus::Pending)
            .count();
        let in_progress = steps
            .iter()
            .filter(|s| s.status == PlanStepStatus::InProgress)
            .count();
        let blocked = steps
            .iter()
            .filter(|s| s.status == PlanStepStatus::Blocked)
            .count();
        let awaiting_approval = steps
            .iter()
            .filter(|s| s.status == PlanStepStatus::AwaitingApproval)
            .count();
        let failed = steps
            .iter()
            .filter(|s| s.status == PlanStepStatus::Failed)
            .count();

        Ok(PlanStatus {
            phase: self.phase,
            total_steps: steps.len(),
            completed,
            pending,
            in_progress,
            blocked,
            awaiting_approval,
            failed,
            waves,
            can_transition: self.is_phase_approved(self.phase),
        })
    }

    /// Reload cache from store if dirty
    fn reload_cache_if_needed(&mut self) -> Result<()> {
        if self.cache_dirty {
            self.steps_cache.clear();
            let tasks = self.store.list(None)?;

            for task in tasks {
                let wave = self.calculate_wave(&task.id, &task.blocked_by)?;

                // Extract metadata fields
                let requires_approval = task
                    .metadata
                    .get("requires_approval")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let acceptance_criteria = task
                    .metadata
                    .get("acceptance_criteria")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                let approval_granted = task
                    .metadata
                    .get("approval_granted")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let status = if requires_approval && !approval_granted {
                    PlanStepStatus::AwaitingApproval
                } else if task.status == TaskStatus::Pending && !task.blocked_by.is_empty() {
                    PlanStepStatus::Blocked
                } else {
                    PlanStepStatus::from(task.status)
                };

                let step = PlanStep {
                    id: task.id,
                    subject: task.subject,
                    description: task.description,
                    active_form: task.active_form,
                    acceptance_criteria,
                    depends_on: task.blocked_by,
                    blocks: task.blocks,
                    status,
                    wave,
                    requires_approval,
                    approval_granted,
                    metadata: task.metadata,
                    created_at: task.created_at,
                    updated_at: task.updated_at,
                };

                self.steps_cache.insert(step.id.clone(), step);
            }

            self.cache_dirty = false;
        }

        Ok(())
    }
}

/// Status summary for a plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStatus {
    /// Current phase
    pub phase: PlanPhase,
    /// Total number of steps
    pub total_steps: usize,
    /// Number of completed steps
    pub completed: usize,
    /// Number of pending steps
    pub pending: usize,
    /// Number of steps in progress
    pub in_progress: usize,
    /// Number of blocked steps
    pub blocked: usize,
    /// Number of steps awaiting approval
    pub awaiting_approval: usize,
    /// Number of failed steps
    pub failed: usize,
    /// Execution waves
    pub waves: Vec<Wave>,
    /// Whether current phase is approved for transition
    pub can_transition: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_plan() -> Result<PlanMode> {
        // Create a PlanMode with an actual in-memory TaskStore
        let store = TaskStore::in_memory()?;
        Ok(PlanMode {
            phase: PlanPhase::Research,
            store,
            approval_gates: {
                let mut gates = HashMap::new();
                gates.insert(
                    PlanPhase::Research,
                    ApprovalGate {
                        phase: PlanPhase::Research,
                        approved: false,
                        notes: None,
                        approved_at: None,
                    },
                );
                gates.insert(
                    PlanPhase::Planning,
                    ApprovalGate {
                        phase: PlanPhase::Planning,
                        approved: false,
                        notes: None,
                        approved_at: None,
                    },
                );
                gates.insert(
                    PlanPhase::Implementation,
                    ApprovalGate {
                        phase: PlanPhase::Implementation,
                        approved: false,
                        notes: None,
                        approved_at: None,
                    },
                );
                gates.insert(
                    PlanPhase::Verification,
                    ApprovalGate {
                        phase: PlanPhase::Verification,
                        approved: false,
                        notes: None,
                        approved_at: None,
                    },
                );
                gates
            },
            steps_cache: HashMap::new(),
            cache_dirty: true,
        })
    }

    #[test]
    fn test_phase_transitions() {
        let mut plan = create_test_plan().unwrap();
        assert_eq!(plan.phase(), PlanPhase::Research);

        // Research -> Planning requires approval
        assert!(plan.transition_phase(None).is_err());

        plan.approve_phase(Some("Research complete".into()))
            .unwrap();
        assert_eq!(plan.transition_phase(None).unwrap(), PlanPhase::Planning);
    }

    #[test]
    fn test_phase_sequence() {
        let mut plan = create_test_plan().unwrap();

        // Research -> Planning
        plan.approve_phase(Some("Research done".into())).unwrap();
        plan.transition_phase(None).unwrap();
        assert_eq!(plan.phase(), PlanPhase::Planning);

        // Planning -> Implementation
        plan.approve_phase(Some("Plan approved".into())).unwrap();
        plan.transition_phase(None).unwrap();
        assert_eq!(plan.phase(), PlanPhase::Implementation);

        // Implementation -> Verification
        plan.approve_phase(Some("Implementation done".into()))
            .unwrap();
        plan.transition_phase(None).unwrap();
        assert_eq!(plan.phase(), PlanPhase::Verification);

        // Verification is terminal
        assert!(plan.transition_phase(None).is_err());
    }

    #[test]
    fn test_create_step() {
        let mut plan = create_test_plan().unwrap();

        let step = plan
            .create_step(CreateStepInput {
                subject: "Test step".to_string(),
                description: "Description".to_string(),
                active_form: Some("Testing".into()),
                acceptance_criteria: vec!["Criterion 1".into(), "Criterion 2".into()],
                depends_on: vec![],
                requires_approval: false,
                metadata: HashMap::new(),
            })
            .unwrap();

        assert_eq!(step.subject, "Test step");
        assert_eq!(step.wave, 0);
        assert_eq!(step.status, PlanStepStatus::Pending);
        assert!(!step.requires_approval);
    }

    #[test]
    fn test_step_with_dependencies() {
        let mut plan = create_test_plan().unwrap();

        // Create independent step
        let step1 = plan
            .create_step(CreateStepInput {
                subject: "Step 1".to_string(),
                description: "First step".to_string(),
                active_form: None,
                acceptance_criteria: vec![],
                depends_on: vec![],
                requires_approval: false,
                metadata: HashMap::new(),
            })
            .unwrap();

        assert_eq!(step1.wave, 0);
        assert_eq!(step1.status, PlanStepStatus::Pending);

        // Create dependent step
        let step2 = plan
            .create_step(CreateStepInput {
                subject: "Step 2".to_string(),
                description: "Second step".to_string(),
                active_form: None,
                acceptance_criteria: vec![],
                depends_on: vec![step1.id.clone()],
                requires_approval: false,
                metadata: HashMap::new(),
            })
            .unwrap();

        assert_eq!(step2.wave, 1); // Wave 1 because depends on step1 (wave 0)
        assert_eq!(step2.status, PlanStepStatus::Blocked);
    }

    #[test]
    fn test_wave_calculation() {
        let mut plan = create_test_plan().unwrap();

        // Create chain: step1 -> step2 -> step3
        let step1 = plan
            .create_step(CreateStepInput {
                subject: "Step 1".to_string(),
                description: "First".to_string(),
                active_form: None,
                acceptance_criteria: vec![],
                depends_on: vec![],
                requires_approval: false,
                metadata: HashMap::new(),
            })
            .unwrap();

        let step2 = plan
            .create_step(CreateStepInput {
                subject: "Step 2".to_string(),
                description: "Second".to_string(),
                active_form: None,
                acceptance_criteria: vec![],
                depends_on: vec![step1.id.clone()],
                requires_approval: false,
                metadata: HashMap::new(),
            })
            .unwrap();

        let step3 = plan
            .create_step(CreateStepInput {
                subject: "Step 3".to_string(),
                description: "Third".to_string(),
                active_form: None,
                acceptance_criteria: vec![],
                depends_on: vec![step2.id.clone()],
                requires_approval: false,
                metadata: HashMap::new(),
            })
            .unwrap();

        assert_eq!(step1.wave, 0);
        assert_eq!(step2.wave, 1);
        assert_eq!(step3.wave, 2);

        let waves = plan.calculate_waves().unwrap();
        assert_eq!(waves.len(), 3);
        assert_eq!(waves[0].step_ids.len(), 1);
        assert_eq!(waves[1].step_ids.len(), 1);
        assert_eq!(waves[2].step_ids.len(), 1);
    }

    #[test]
    fn test_step_with_approval() {
        let mut plan = create_test_plan().unwrap();

        let step = plan
            .create_step(CreateStepInput {
                subject: "Sensitive step".to_string(),
                description: "Requires approval".to_string(),
                active_form: None,
                acceptance_criteria: vec![],
                depends_on: vec![],
                requires_approval: true,
                metadata: HashMap::new(),
            })
            .unwrap();

        assert_eq!(step.status, PlanStepStatus::AwaitingApproval);
        assert!(step.requires_approval);
        assert!(!step.approval_granted);

        // Approve the step
        let approved_step = plan.approve_step(&step.id).unwrap().unwrap();
        assert_eq!(approved_step.status, PlanStepStatus::Pending);
        assert!(approved_step.approval_granted);
    }

    #[test]
    fn test_step_status_update() {
        let mut plan = create_test_plan().unwrap();

        let step = plan
            .create_step(CreateStepInput {
                subject: "Test step".to_string(),
                description: "Description".to_string(),
                active_form: None,
                acceptance_criteria: vec![],
                depends_on: vec![],
                requires_approval: false,
                metadata: HashMap::new(),
            })
            .unwrap();

        // Start working on step
        let updated = plan
            .update_step_status(&step.id, PlanStepStatus::InProgress)
            .unwrap()
            .unwrap();
        assert_eq!(updated.status, PlanStepStatus::InProgress);

        // Complete step
        let completed = plan
            .update_step_status(&step.id, PlanStepStatus::Completed)
            .unwrap()
            .unwrap();
        assert_eq!(completed.status, PlanStepStatus::Completed);
    }

    #[test]
    fn test_dependent_step_unblocks() {
        let mut plan = create_test_plan().unwrap();

        let step1 = plan
            .create_step(CreateStepInput {
                subject: "Step 1".to_string(),
                description: "First".to_string(),
                active_form: None,
                acceptance_criteria: vec![],
                depends_on: vec![],
                requires_approval: false,
                metadata: HashMap::new(),
            })
            .unwrap();

        let step2 = plan
            .create_step(CreateStepInput {
                subject: "Step 2".to_string(),
                description: "Second".to_string(),
                active_form: None,
                acceptance_criteria: vec![],
                depends_on: vec![step1.id.clone()],
                requires_approval: false,
                metadata: HashMap::new(),
            })
            .unwrap();

        assert_eq!(step2.status, PlanStepStatus::Blocked);

        // Complete step1
        plan.update_step_status(&step1.id, PlanStepStatus::Completed)
            .unwrap();

        // Check that step2 is now pending
        let step2_updated = plan.get_step(&step2.id).unwrap().unwrap();
        assert_eq!(step2_updated.status, PlanStepStatus::Pending);
    }

    #[test]
    fn test_status_summary() {
        let mut plan = create_test_plan().unwrap();

        // Create multiple steps
        let _ = plan
            .create_step(CreateStepInput {
                subject: "Step 1".to_string(),
                description: "First".to_string(),
                active_form: None,
                acceptance_criteria: vec![],
                depends_on: vec![],
                requires_approval: false,
                metadata: HashMap::new(),
            })
            .unwrap();
        let step2 = plan
            .create_step(CreateStepInput {
                subject: "Step 2".to_string(),
                description: "Second".to_string(),
                active_form: None,
                acceptance_criteria: vec![],
                depends_on: vec![],
                requires_approval: false,
                metadata: HashMap::new(),
            })
            .unwrap();
        let _step3 = plan
            .create_step(CreateStepInput {
                subject: "Step 3".to_string(),
                description: "Third".to_string(),
                active_form: None,
                acceptance_criteria: vec![],
                depends_on: vec![],
                requires_approval: true,
                metadata: HashMap::new(),
            })
            .unwrap();

        // Update some statuses
        plan.update_step_status(&step2.id, PlanStepStatus::Completed)
            .unwrap();

        let status = plan.status().unwrap();
        assert_eq!(status.total_steps, 3);
        assert_eq!(status.completed, 1);
        assert_eq!(status.pending, 1);
        assert_eq!(status.awaiting_approval, 1);
    }
}
