# Claude Code Instructions

Project-specific guidelines for Claude Code when working on this TI-84 Plus CE emulator.

## Code Style

- When leaving functionality for future implementation, always add a `TODO:` comment explaining what needs to be done and which milestone it's planned for
  - Example: `// TODO: Wire up BusFault when Bus reports invalid memory access (Milestone 5+)`
- Keep TODO comments concise but include enough context to understand the task

## Testing

- In Z80 mode tests, remember to set `cpu.mbase = 0x00` when poking bytes at address 0, since the default MBASE (0xD0) causes fetches from 0xD00000
- Use minimal ROM buffers in tests - flash defaults to 0xFF, so only include the bytes actually needed

## Architecture

- See [docs/architecture.md](docs/architecture.md) for system design
- See [docs/milestones.md](docs/milestones.md) for implementation roadmap
