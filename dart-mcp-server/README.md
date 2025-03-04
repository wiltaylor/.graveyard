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
- [Tools](#tools)
- [Setup](#setup)
  - [Authentication token](#authentication-token)
  - [Usage with MCP clients](#usage-with-mcp-clients)
    - [Docker](#docker)
  - [NPX](#npx)
- [Help and Resources](#help-and-resources)
- [Contributing](#contributing)
- [License](#license)

## Features

<!-- TODO -->

## Tools

<!-- TODO -->

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
