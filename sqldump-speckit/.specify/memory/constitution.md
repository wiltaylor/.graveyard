<!--
Sync Impact Report (2025-10-14):
─────────────────────────────────────────────────────────────────────────────
Version Change: [No previous version] → 1.0.0

Changes Made:
  ✓ Initial constitution creation
  ✓ Defined 7 core principles for SpecKit framework
  ✓ Established specification-first development workflow
  ✓ Set quality gates and governance model

Principles Established:
  + I. Specification-First Development (NON-NEGOTIABLE)
  + II. User-Centric Design
  + III. Template-Driven Consistency
  + IV. Phased Implementation Planning
  + V. Independent Story Implementation
  + VI. AI-Assisted Development
  + VII. Quality Gates & Validation

Templates Requiring Updates:
  ✅ plan-template.md (already aligned with constitution principles)
  ✅ spec-template.md (already aligned with user story requirements)
  ✅ tasks-template.md (already aligned with phased/story-based approach)
  ✅ All prompt files (inherently reference constitution workflow)

Follow-up Actions:
  - None required; initial version is complete and synchronized
─────────────────────────────────────────────────────────────────────────────
-->

# SpecKit Constitution

## Core Principles

### I. Specification-First Development (NON-NEGOTIABLE)

Every feature MUST begin with a complete specification before any implementation work begins. 
The specification must include:
- Clearly defined user scenarios with acceptance criteria
- Prioritized, independently testable user stories
- Functional requirements that are measurable and technology-agnostic
- Success criteria with both quantitative and qualitative measures

**Rationale**: Specifications prevent scope creep, enable parallel work, create shared 
understanding between humans and AI agents, and serve as the source of truth for validation.

**Gate**: No planning or implementation may proceed without an approved specification in 
`/specs/[###-feature-name]/spec.md`.

### II. User-Centric Design

Requirements MUST be expressed as user journeys and acceptance scenarios, not technical 
implementations. Each user story must be:
- Independently valuable (delivers standalone value)
- Independently testable (can be verified in isolation)
- Priority-ranked (P1, P2, P3...) to enable MVP-first delivery

**Rationale**: User-centric design ensures features solve real problems, enables incremental 
delivery, and allows stakeholders to validate value early without waiting for complete 
implementations.

**Gate**: Specifications lacking clear user scenarios or with non-independent stories must 
be rejected during the specification review phase.

### III. Template-Driven Consistency

All artifacts (specifications, plans, tasks, checklists) MUST follow their respective 
templates located in `.specify/templates/`. Templates ensure:
- Consistent structure across all features and team members
- Required sections are never omitted
- AI agents can reliably parse and generate artifacts
- New team members have clear patterns to follow

**Rationale**: Consistency reduces cognitive load, enables automation, and creates 
predictable workflows that both humans and AI can optimize.

**Gate**: Any artifact that deviates from its template structure without documented 
justification must be rejected.

### IV. Phased Implementation Planning

Implementation plans MUST follow the three-phase approach:
- **Phase 0 (Research)**: Resolve all technical unknowns, research best practices, 
  document technology decisions
- **Phase 1 (Design)**: Define data models, generate API contracts, create quickstart 
  guides, update agent context
- **Phase 2 (Implementation)**: Break work into prioritized, independently executable 
  tasks organized by user story

**Rationale**: Phased planning prevents implementation surprises, enables parallel research, 
front-loads risk discovery, and ensures complete design before coding begins.

**Gate**: Implementation cannot begin until Phase 1 artifacts (data-model.md, contracts/, 
quickstart.md) are complete and reviewed.

### V. Independent Story Implementation

Each user story MUST be implementable as a standalone unit that delivers value independently. 
Tasks must be organized by story, not by technical layer. Stories must be ordered by 
priority (P1 first) to enable:
- MVP delivery after completing only P1 stories
- Parallel development by multiple developers/agents
- Early user feedback on high-priority features
- Graceful feature cancellation (drop P3 stories without breaking P1/P2)

**Rationale**: Independent stories reduce dependencies, enable flexible resource allocation, 
support continuous delivery, and align development with business priorities.

**Gate**: Any task list that cannot demonstrate story independence must be restructured 
before implementation approval.

### VI. AI-Assisted Development

The SpecKit framework is designed for seamless human-AI collaboration. All commands and 
workflows MUST:
- Accept natural language input from users
- Generate structured, parseable output (JSON + human-readable)
- Operate idempotently (safe to re-run)
- Use absolute paths for all file operations
- Update agent-specific context files (e.g., `.github/copilot-instructions.md`) 
  automatically

**Rationale**: AI agents excel at pattern-based work (generating from templates, 
researching best practices, maintaining consistency) while humans excel at creative 
decisions and priority judgment. The framework optimizes for this division of labor.

**Gate**: Any script or command that requires manual file path resolution or produces 
ambiguous output must be refactored.

### VII. Quality Gates & Validation

Every workflow phase has mandatory gates that MUST be passed before proceeding:
- **Specification Gate**: User scenarios testable, requirements measurable, success 
  criteria defined
- **Constitution Gate**: No violations of core principles unless explicitly justified 
  in complexity tracking
- **Design Gate**: All "NEEDS CLARIFICATION" markers resolved, data model complete, 
  contracts generated
- **Implementation Gate**: Tests written (if applicable), tests fail, then implementation 
  proceeds

**Rationale**: Gates prevent downstream problems, enforce discipline, create natural 
review points, and ensure quality is built in, not inspected in later.

**Gate**: Attempting to skip a gate or proceed with unresolved blockers is a 
constitutional violation.

## Quality Standards

### Clarity Over Brevity

Documentation MUST prioritize clarity and completeness over terseness. Every template 
includes examples and comments explaining intent. Specifications should include:
- Explicit assumptions when making reasonable defaults
- Rationale for priority rankings
- Context for why features are needed (not just what they do)

**Enforcement**: Any artifact requiring interpretation or clarification indicates 
insufficient detail.

### Absolute Paths Required

All scripts, commands, and file references MUST use absolute paths. Relative paths create 
ambiguity about execution context and break automation.

**Enforcement**: Script validation will reject relative path usage.

### Error Handling Standards

All scripts and workflows MUST:
- Return actionable error messages (not just failure codes)
- Use ERROR prefix for blocking issues
- Suggest remediation steps in error output
- Exit with appropriate codes (0=success, 1+=failure severity)

**Enforcement**: Any script that fails silently or with cryptic errors must be improved.

## Development Workflow

### Feature Lifecycle

1. **Specify** (`/speckit.specify`): User describes feature → generates spec.md
2. **Plan** (`/speckit.plan`): Research unknowns → design models & contracts → update 
   agent context
3. **Task** (`/speckit.tasks`): Break design into story-organized implementation tasks
4. **Implement** (`/speckit.implement`): Execute tasks, pass gates, deliver stories
5. **Validate**: Verify against original specification acceptance criteria

### Branch Naming Convention

All feature work MUST use the format: `[###-feature-name]` where `###` is a unique numeric 
identifier (e.g., `001-user-authentication`, `042-payment-processing`).

**Rationale**: Numeric prefixes enable sorting, guarantee uniqueness, and create 
bidirectional traceability between specs, branches, and deliverables.

### Agent Context Updates

After Phase 1 design, the system MUST automatically update agent-specific instruction 
files with:
- New technologies/frameworks introduced by this feature
- Design patterns established in contracts
- Testing approaches required

**Location**: `.github/copilot-instructions.md` (GitHub Copilot), or equivalent for other 
AI agents.

**Enforcement**: The `update-agent-context.sh` script is mandatory in the plan workflow.

## Governance

### Constitutional Authority

This constitution supersedes all other development practices, style guides, or informal 
conventions. When conflicts arise, the constitution takes precedence unless an explicit 
amendment process has been initiated.

### Amendment Process

Constitutional changes require:
1. Documented proposal explaining the need for change
2. Impact analysis on existing templates, workflows, and active features
3. Version increment following semantic versioning rules
4. Sync Impact Report documenting all affected artifacts
5. Approval from project maintainers (or user/stakeholder if solo project)

### Versioning Policy

Constitution versions follow **MAJOR.MINOR.PATCH** semantics:
- **MAJOR**: Principle removal/redefinition, backward-incompatible workflow changes
- **MINOR**: New principle added, expanded guidance, new quality standards
- **PATCH**: Clarifications, typo fixes, example improvements, non-semantic refinements

### Complexity Justification

Violations of core principles MUST be justified in the "Complexity Tracking" section of 
the implementation plan with:
- Which principle is being violated and why
- What alternative approaches were considered
- Why this violation is necessary for feature success
- Mitigation plan to minimize technical debt

**Enforcement**: Unjustified complexity is grounds for implementation rejection.

### Review Checkpoints

All feature work must pass these reviews:
- **Specification Review**: Verify user stories, requirements, testability
- **Constitution Review**: Check gates, validate no unjustified violations
- **Design Review**: Validate data model, contracts, Phase 1 completeness
- **Implementation Review**: Verify tests (if applicable), story independence, quality

**Version**: 1.0.0 | **Ratified**: 2025-10-14 | **Last Amended**: 2025-10-14