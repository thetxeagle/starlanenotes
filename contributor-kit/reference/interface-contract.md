# Public Starlane Notes interface contract

This contributor edition summarizes the applicable Starlane suite and Notes rules. It is maintained
with the public skill kit; it does not require private contracts or governance tooling.

## Priority and shared ownership

Product intent comes first, followed by accessibility, security, data integrity and platform behavior,
then brand/semantic tokens, shared components, Notes workflows and craft defaults. Document narrow
exceptions with a reason and evidence; exceptions cannot waive accessibility or data safety.
Inspect existing shared components in [apps/web/src/components](../../apps/web/src/components) before
adding controls. Reuse Starlane components, then approved shadcn/Radix primitives. DialogShell owns
focus containment, dismissal and restoration; ConfirmDialog owns consequential confirmations.
Do not replace them with browser alert/confirm/prompt or hand-rolled overlays.

## Tokens, themes and visual hierarchy

Use existing semantic tokens and scales; do not hardcode new palettes or one-off control styling.
Maintain dark Void Space and light Cloud Canvas across default, hover, focus, active, selected,
disabled, loading, empty, success, warning and error states where relevant.

| Token | Meaning |
| --- | --- |
| `--bg`, `--surface`, `--surface-raised` | Canvas, grouped content, layered controls |
| `--border` | Boundaries and dividers |
| `--text`, `--text-muted` | Primary and secondary content |
| `--primary` | Notes blue actions, links, focus and selection |
| `--ai` | Purple AI entry points and surfaces |
| `--accent`, `--warning` | Attention/notification and caution |
| `--success` | Green connected or successful state |
| `--danger` | Red destructive actions and errors |

No glassmorphism, decorative gradients/glows, colored shadows, floating structural navigation,
pill-shaped primary actions/navigation, ornamental hero filler, fake charts or decorative hover
movement. Pills may represent tags/statuses/filters. Use layout, typography and restrained surfaces
for hierarchy; preserve readable prose width. Select existing spacing, radius and elevation tokens
in context. Motion must explain state, remain interruptible and respect reduced motion.
Lucide is the action icon lane; preserve stored content emoji and validated custom content icons.
Icons inherit text color unless a semantic status owns the color.

## Notes workflow boundaries

The sidebar owns space selection, Search and hierarchy. The document owns its page header and one
persistent Edit toolbar. Read mode hides editing tools. Contents navigation is separate from
inspection; avoid prose jumps when panels open. Keep common actions discoverable without hover or
shortcut knowledge. Preserve equivalent information and actions on phone/tablet and native shells.

Saved draft work and the published page are different states. Inspect current document, draft,
access and offline behavior before changing navigation, save or publish flows. Do not imply that
local save grants audience access or that queued offline work reached the server. Preserve resume,
permissions, recovery, links and stored identity; do not simplify behavior just for a screenshot.
Account/profile actions stay distinct from workspace settings and permission-gated administration.

## Evidence and review

Run the design validator and affected tests. Render the changed journey with fictional content in
both themes, laptop and phone widths; include tablet/large desktop when layout changes. Check keyboard
focus/dismissal, names, zoom, long text, overflow, touch targets and reduced motion. Exercise affected
loading/error/offline states and confirm recovery preserves work. For editor work include relevant
headings, lists, links, code, tables, undo/redo, reload and panel transitions.
Record exact coverage and label unavailable browser/device/provider checks Not verified. Static code
checks cannot establish optical balance, clipping, touch feel or real deployment success.
