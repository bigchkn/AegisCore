# LLD: Web UI Taskflow Enhancements

## Overview
Enhance the Taskflow view in the Web UI to provide more context (milestone titles), support for the global backlog (bugs/maintenance), and task filtering.

## Goals
- Show milestone titles in the main list.
- Display the global backlog as a first-class citizen.
- Add a filter toggle: "All" vs "Incomplete" (default: Incomplete).
- Improve visual distinction between different task types (Features, Bugs, Maintenance).

## Architecture Changes

### Backend (aegis-taskflow)
- Update `MilestoneRef` struct in `model.rs` to include a `name` field.
- Update `TaskflowEngine` to populate `name` in `MilestoneRef` when reading the index (or ensure it's saved in `index.toml`).
- *Self-correction:* `MilestoneRef` is stored in `index.toml`. We should include `name` there so we don't have to read every milestone file just to show the list.

### Frontend (aegis-web)
- **Store Updates (`domain.ts`):**
    - Update `MilestoneRef` type.
    - Add `TaskType` to `ProjectTask` (already there in backend, ensure it's in frontend).
- **View Updates (`TaskflowView.tsx`):**
    - Render milestone names in the list.
    - Include "backlog" in the list if present in `TaskflowIndex`.
    - Add a `filter` state (enum: `All` | `Incomplete`).
    - Default filter to `Incomplete`.
    - Filter tasks within milestones/backlog based on state.
    - Add CSS for different task types (e.g., bug icon/color).

## Implementation Plan

### Phase 1: Model & Data
1.  Update `MilestoneRef` in `crates/aegis-taskflow/src/model.rs`.
2.  Update `create_milestone` logic in `aegis-taskflow` to include `name` in `index.toml`.
3.  Migration: Manually update `index.toml` for existing milestones.

### Phase 2: Frontend Infrastructure
1.  Update `domain.ts` in frontend to match backend changes.

### Phase 3: Frontend UI
1.  Update `TaskflowView.tsx`:
    - Add filtering logic.
    - Add "Show All" / "Hide Completed" toggle.
    - Show milestone names.
    - Include Backlog in the list.
    - Distinguish task types (Bug, Maintenance, Feature).
