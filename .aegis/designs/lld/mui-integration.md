# LLD: Material UI Integration & UX Improvements

## Overview
This document outlines the low-level design for integrating Material UI (MUI) into AegisCore's web interface. The goal is to provide a modern, responsive shell with a collapsible sidebar and improved UX consistency.

## Component Architecture

### Core Shell (`App.tsx`)
- The `app-shell` will be refactored to use MUI's `Box` and `Stack` for layout.
- `CssBaseline` will be used to normalize styles across browsers.
- `ThemeProvider` will inject the custom Aegis theme.

### Sidebar (`Sidebar.tsx`)
- Will be converted to a `MUI Drawer` (persistent variant for desktop, temporary for mobile).
- State for collapse (`isSidebarOpen`) will be managed in `uiSlice` (Redux) to ensure it can be toggled from anywhere (e.g., the top bar).
- Widths: 240px (expanded), 64px (mini variant).

### Header (App Bar)
- A new `AppBar` component will replace the current `header.topbar`.
- Contains:
  - Sidebar toggle button (Menu icon).
  - Breadcrumbs/View title.
  - Project path.
  - Connection status pill (MUI `Chip`).

## Theme Configuration (`theme.ts`)
- **Palette:** Dark mode by default.
  - Primary: Aegis Purple (e.g., `#9c27b0`).
  - Background: Dark gray/Black (e.g., `#121212`).
- **Typography:** Inter or system default.
- **Components:** Custom overrides for `Drawer` and `Button` to match the industrial/developer tool aesthetic.

## State Management
- `uiSlice.ts` will be extended:
  ```typescript
  interface UIState {
    sidebarOpen: boolean;
    // ... existing fields
  }
  ```
- Actions: `toggleSidebar`, `setSidebarOpen`.

## Task Breakdown

### Phase 1: Infrastructure
1. Install dependencies.
2. Create `theme.ts`.
3. Wrap `App.tsx` with `ThemeProvider`.
4. Update `uiSlice.ts` with `sidebarOpen` state.

### Phase 2: Sidebar & Header
1. Refactor `Sidebar.tsx` to MUI Drawer.
2. Implement mini-variant (icons only) when collapsed.
3. Create `TopBar.tsx` using MUI `AppBar`.
4. Add toggle functionality.

### Phase 3: Incremental View Updates
1. Update `AgentsView` with MUI list/grid components.
2. Update `StatusBadge` to use MUI `Chip`.
3. General layout cleanup (spacing, typography).
