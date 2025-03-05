<div align="center">
  <h1>Dart MCP Server</h1>
  <p>
    <a href="https://pypi.org/project/dart-mcp-server"><img src="https://img.shields.io/pypi/v/dart-mcp-server" alt="PyPI"></a>
    <a href="pyproject.toml"><img src="https://img.shields.io/pypi/pyversions/dart-mcp-server" alt="Supported Python Versions"></a>
    <a href="LICENSE"><img src="https://img.shields.io/github/license/its-dart/dart-mcp-server" alt="License"></a>
  </p>
</div>

[Dart](https://itsdart.com?nr=1) is Project Management powered by AI.

`dart-mcp-server` is the official AI [Model Context Protocol (MCP)](https://github.com/modelcontextprotocol) server for Dart.

- [Features](#features)
  - [Prompts](#prompts)
  - [Resource templates](#resource-templates)
  - [Tools](#tools)
    - [Task management](#task-management)
    - [Document management](#document-management)
- [Setup](#setup)
  - [Authentication token](#authentication-token)
  - [Usage with MCP clients](#usage-with-mcp-clients)
    - [Docker](#docker)
  - [NPX](#npx)
- [Help and Resources](#help-and-resources)
- [Contributing](#contributing)
- [License](#license)

## Features

### Prompts

The following prompts are available

- `create-task` - Create a new task in Dart with title, description, status, priority, and assignee
- `create-doc` - Create a new document in Dart with title, text content, and folder
- `summarize-tasks` - Get a summary of tasks with optional filtering by status and assignee

These prompts make it easy for AI assistants to perform common actions in Dart without needing to understand the underlying API details.

### Resource templates

The following resources are available

- `dart-config:` - Configuration information about the user's space
- `dart-task:///{taskId}` - Detailed information about specific tasks
- `dart-doc:///{docId}` - Detailed information about specific docs

### Tools

The following tools are available

#### Task management

- `get_config` - Get information about the user's space, including available assignees, dartboards, folders, statuses, tags, priorities, and sizes
- `list_tasks` - List tasks with optional filtering by assignee, status, dartboard, priority, due date, and more
- `create_task` - Create a new task with title, description, status, priority, size, dates, dartboard, assignees, tags, and parent task
- `get_task` - Retrieve an existing task by its ID
- `update_task` - Update an existing task's properties
- `delete_task` - Move a task to the trash (recoverable)

#### Document management

- `list_docs` - List docs with optional filtering by folder, title, text content, and more
- `create_doc` - Create a new doc with title, text content, and folder
- `get_doc` - Retrieve an existing doc by its ID
- `update_doc` - Update an existing doc's properties
- `delete_doc` - Move a doc to the trash (recoverable)

Each tool supports comprehensive input validation and returns structured JSON responses.

## Setup

### Authentication token

Copy your authentication token from [your Dart profile](https://app.itsdart.com/?settings=account) and use that below.

### Usage with MCP clients

#### Docker

Run

```bash
docker build -t mcp/dart .
```

To use this with Claude Desktop, add the following to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "dart": {
      "command": "docker",
      "args": ["run", "-i", "--rm", "-e", "DART_TOKEN", "mcp/dart"],
      "env": {
        "DART_TOKEN": "dsa_..."
      }
    }
  }
}
```

### NPX

To use this with Claude Desktop, add the following to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "dart": {
      "command": "npx",
      "args": ["-y", "dart-mcp-server"],
      "env": {
        "DART_TOKEN": "dsa_..."
      }
    }
  }
}
```

## Help and Resources

- [Homepage](https://www.itsdart.com/?nr=1)
- [Web App](https://app.itsdart.com/)
- [Help Center](https://help.itsdart.com/)
- [Bugs and Features](https://app.itsdart.com/p/r/JFyPnhL9En61)
- [Library Source](https://github.com/its-dart/dart-mcp-server/)
- [Chat on Discord](https://discord.gg/RExv8jEkSh)
- Email us at [support@itsdart.com](mailto:support@itsdart.com)

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.

## License

This project is licensed under [the MIT License](LICENSE).
