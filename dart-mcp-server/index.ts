#!/usr/bin/env node

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  ListToolsRequestSchema,
  CallToolRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import axios from "axios";
import { z } from "zod";
import dotenv from "dotenv";
import { readFileSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

dotenv.config();

const dartToken = process.env.DART_TOKEN;
if (!dartToken) {
  console.error("DART_TOKEN environment variable is required");
  process.exit(1);
}
const dartHost = process.env.DART_HOST || "https://app.itsdart.com";

const headers = {
  Authorization: `Bearer ${dartToken}`,
};

const filename = fileURLToPath(import.meta.url);
const packageJson = JSON.parse(
  readFileSync(join(dirname(filename), "..", "package.json"), "utf-8"),
);

const TaskListParamsSchema = z.object({
  assignee: z.string().optional(),
  assignee_duid: z.string().optional(),
  dartboard: z.string().optional(),
  dartboard_duid: z.string().optional(),
  description: z.string().optional(),
  due_at: z.string().optional(),
  duids: z.string().optional(),
  in_trash: z.boolean().optional(),
  is_draft: z.boolean().optional(),
  kind: z.string().optional(),
  limit: z.number().optional(),
  offset: z.number().optional(),
  priority: z.string().optional(),
  size: z.number().optional(),
  start_at: z.string().optional(),
  status: z.string().optional(),
  status_duid: z.string().optional(),
  subscriber_duid: z.string().optional(),
  tag: z.string().optional(),
  title: z.string().optional(),
});

const server = new Server(
  {
    name: "dart-mcp",
    version: packageJson.version,
  },
  {
    capabilities: {
      tools: {},
    },
  },
);

server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: [
    {
      name: "list_tasks",
      description:
        "List tasks from Dart with optional filtering parameters. You can filter by assignee, status, dartboard, priority, due date, and more.",
      inputSchema: {
        type: "object",
        properties: {
          assignee: {
            type: "string",
            description: "Filter by assignee name or email",
          },
          assignee_duid: {
            type: "string",
            description: "Filter by assignee DUID",
          },
          dartboard: {
            type: "string",
            description: "Filter by dartboard title",
          },
          dartboard_duid: {
            type: "string",
            description: "Filter by dartboard DUID",
          },
          description: {
            type: "string",
            description: "Filter by description content",
          },
          due_at: {
            type: "string",
            description: "Filter by due date (ISO format)",
          },
          duids: { type: "string", description: "Filter by DUIDs" },
          in_trash: { type: "boolean", description: "Filter by trash status" },
          is_draft: { type: "boolean", description: "Filter by draft status" },
          kind: { type: "string", description: "Filter by task kind" },
          limit: { type: "number", description: "Number of results per page" },
          offset: {
            type: "number",
            description: "Initial index for pagination",
          },
          priority: { type: "string", description: "Filter by priority" },
          size: { type: "number", description: "Filter by task size" },
          start_at: {
            type: "string",
            description: "Filter by start date (ISO format)",
          },
          status: { type: "string", description: "Filter by status" },
          status_duid: { type: "string", description: "Filter by status DUID" },
          subscriber_duid: {
            type: "string",
            description: "Filter by subscriber DUID",
          },
          tag: { type: "string", description: "Filter by tag" },
          title: { type: "string", description: "Filter by title" },
        },
        required: [],
      },
    },
  ],
}));

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  try {
    if (!request.params.arguments) {
      throw new Error("Arguments are required");
    }

    switch (request.params.name) {
      case "list_tasks": {
        const params = TaskListParamsSchema.parse(request.params.arguments);
        const response = await axios.get(
          `${dartHost}/api/v0/chatgpt/tasks/list`,
          { headers, params },
        );

        return {
          content: [
            { type: "text", text: JSON.stringify(response.data, null, 2) },
          ],
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
