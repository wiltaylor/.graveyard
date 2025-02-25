#!/usr/bin/env node

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { ListToolsRequestSchema, CallToolRequestSchema } from "@modelcontextprotocol/sdk/types.js";
import axios from "axios";
import { z } from 'zod';
import dotenv from "dotenv";
import { readFileSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from 'url';

dotenv.config();

const dartToken = process.env.DART_TOKEN;
if (!dartToken) {
  console.error("DART_TOKEN environment variable is required");
  process.exit(1);
}

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const packageJson = JSON.parse(
  readFileSync(join(__dirname, "..", "package.json"), "utf-8")
);

const server = new Server(
  {
    name: "dart-mcp",
    version: packageJson.version,
  },
  {
    capabilities: {
      tools: {},
    },
  }
);

server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: [{
    name: "list_tasks",
    description: "List all of the Dart tasks",
    inputSchema: {
      type: "object",
      properties: {
        task_id: {
          type: "string"
        }
      },
      required: []
    },
  }]
}));

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  try {
    if (!request.params.arguments) {
      throw new Error("Arguments are required");
    }

    switch (request.params.name) {
      case "list_tasks": {
        console.error("Listing tasks");
        const tasks = await axios.get("http://localhost:8000/api/v0/chatgpt/tasks/list", {
          headers: {
            Authorization: `Bearer ${dartToken}`,
          }
        })
        return {
          content: [{ type: "text", text: JSON.stringify(tasks.data, null, 2) }],
        };
      }
      default:
        throw new Error(`Unknown tool: ${request.params.name}`);
    }
  } catch (error) {
    if (error instanceof z.ZodError) {
      throw new Error(`Invalid input: ${JSON.stringify(error.errors)}`);
    }
    throw error;
  }
});

async function runServer() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error("Dart MCP Server running on stdio");
}

runServer().catch((error) => {
console.error("Unhandled error:", error);
process.exit(1);
});
