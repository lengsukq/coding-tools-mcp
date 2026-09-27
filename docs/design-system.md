# Coding Tools MCP UI Design System

> iOS Glass: a calm frosted-glass desktop with generous rounded corners, clear hierarchy, and readable controls.

## Visual direction

Use an iOS-inspired desktop style with large rounded corners, translucent frosted surfaces, soft layered highlights, and one palette-controlled accent. Keep ambient light subtle and static; avoid rings, animated orbs, and strong gradients that compete with the content. Keep success, warning, and error colors semantic.

## Information hierarchy

1. Current work, risks, errors, and primary actions.
2. Workspace, runtime, connection, and progress state.
3. Activity, analytics, history, reviews, and audit summaries.
4. Paths, IDs, revisions, and other technical metadata.

Keep lower-priority information visually quiet until it helps the current task.

## Layout and spacing

- Use `PageShell`, `PageHeader`, and `SectionHeader` for shared page rhythm.
- Dashboard and Workspace use a wide canvas; Settings use a reading width.
- Use a 4px spacing scale: 4, 8, 12, 16, 24, 32, 48, and 64px. Leave visible breathing room between glass surfaces.
- Keep one clear primary action per task area. Isolate destructive actions.
- Use native `details` / `summary` for advanced or low-frequency settings.
- At narrow widths, keep the task order in one column and remove page-level horizontal scrolling.

## Type and color

- Use the platform UI font stack; use a system monospace font for paths, IDs, and endpoints.
- Page titles: 24–28px. Section titles: 16px. Body: 14px. Regular UI text: at least 12px.
- Neutral colors should cover at least 90% of the canvas.
- Palette choices affect the accent only. They must not change status semantics.
- Pair every status color with text or an icon.

## Surfaces and components

| Component | Rule |
| --- | --- |
| `GlassCard` | Translucent surface with restrained backdrop blur, a soft highlight, and a large corner radius; static cards do not lift on hover. |
| `MetricCard` | A supporting metric, not a primary action. No decorative color treatment. |
| `ActionCard` | Use only when the whole surface is actionable. |
| `DataRow` | Use for compact configuration, endpoint, profile, and list information. |
| `BaseButton` | Primary, secondary, ghost, danger, or icon. Avoid multiple primary actions in one region. |
| `StatusPill` | Fixed semantic colors and visible status text. |
| Form controls | Persistent labels, clear disabled state, and errors beside the affected field. |

Use translucent borders and spacing to separate surfaces, with enough blur and fill opacity to keep text readable. Reserve stronger shadows for dialogs and popovers.

## Page priorities

- **Dashboard:** current work → needs attention → workspace/runtime status → activity → analytics.
- **Workspace:** current plan and risks → Git/runtime/verification → review/history → technical details.
- **Tool Audit:** filters and records → health and retention → compact summary metrics.
- **Settings:** primary configuration → status and save action → collapsed advanced options → technical help.

When there are no audit records, show an unavailable/empty success rate instead of `100%`.

## Interaction and accessibility

- Provide hover, active, focus-visible, and disabled styles for controls.
- Keep keyboard focus visible and preserve native disclosure behavior.
- Support `prefers-reduced-motion`; animate direct state changes only.
- Keep touch targets practical and avoid hiding the only way to complete a task.

## CSS ownership

- `palette.css` defines brand accents only.
- `styles/design-system.css` owns semantic tokens and the current surface/page overrides.
- `styles/ios-vue.css` and `styles/workbench.css` remain compatibility layers; do not add new visual rules there.
- Prefer `--ui-*` tokens and shared components in new UI.
