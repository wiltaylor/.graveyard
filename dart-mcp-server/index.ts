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

const token = process.env.DART_TOKEN;
if (!token) {
  console.error("DART_TOKEN environment variable is required");
  process.exit(1);
}
const hostBase = process.env.DART_HOST || "https://app.itsdart.com";
const host = `${hostBase}/api/v0/chatgpt`;

const headers = {
  Authorization: `Bearer ${token}`,
};

const filename = fileURLToPath(import.meta.url);
const packageJson = JSON.parse(
  readFileSync(join(dirname(filename), "..", "package.json"), "utf-8"),
);

// Common schemas
const AssigneeSchema = z.object({
  name: z.string(),
  email: z.string(),
  duid: z.string(),
});

const TaskSchema = z.object({
  id: z.string(),
  title: z.string(),
  description: z.string().nullable(),
  status: z.string().nullable(),
  priority: z.string().nullable(),
  size: z.number().nullable(),
  start_at: z.string().nullable(),
  due_at: z.string().nullable(),
  created_at: z.string(),
  updated_at: z.string(),
  permalink: z.string(),
  dartboard: z.string().nullable(),
  assignees: z.array(AssigneeSchema),
  tags: z.array(z.string()),
  parent: z.string().nullable(),
  is_draft: z.boolean(),
  in_trash: z.boolean(),
});

// Request schemas
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

const TaskCreateSchema = z.object({
  title: z.string(),
  description: z.string().optional(),
  status: z.string().optional(),
  priority: z.string().optional(),
  size: z.number().optional(),
  start_at: z.string().optional(),
  due_at: z.string().optional(),
  dartboard: z.string().optional(),
  assignees: z.array(z.string()).optional(),
  assignee: z.string().optional(),
  tags: z.array(z.string()).optional(),
  parent: z.string().optional(),
});

const WrappedTaskCreateSchema = z.object({
  item: TaskCreateSchema,
});

// Response schemas
const ConfigResponseSchema = z.object({
  today: z.string(),
  assignees: z.array(AssigneeSchema),
  dartboards: z.array(z.string()),
  folders: z.array(z.string()),
  statuses: z.array(z.string()),
  tags: z.array(z.string()),
  priorities: z.array(z.string()),
  sizes: z.array(z.number()),
});

const TaskListResponseSchema = z.object({
  items: z.array(TaskSchema),
  total: z.number(),
  limit: z.number(),
  offset: z.number(),
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
      name: "get_config",
      description: "Get information about the user's space, including all of the possible values that can be provided to other endpoints. This includes available assignees, dartboards, folders, statuses, tags, priorities, and sizes.",
      inputSchema: {
        type: "object",
        properties: {},
        required: [],
      },
    },
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
    {
      name: "create_task",
      description: "Create a new task in Dart. You can specify title, description, status, priority, size, dates, dartboard, assignees, tags, and parent task.",
      inputSchema: {
        type: "object",
        properties: {
          title: {
            type: "string",
            description: "The title of the task (required)",
          },
          description: {
            type: "string",
            description: "A longer description of the task, which can include markdown formatting",
          },
          status: {
            type: "string",
            description: "The status from the list of available statuses",
          },
          priority: {
            type: "string",
            description: "The priority (Critical, High, Medium, or Low)",
          },
          size: {
            type: "number",
            description: "A number that represents the amount of work needed",
          },
          start_at: {
            type: "string",
            description: "The start date in ISO format (should be at 9:00am in user's timezone)",
          },
          due_at: {
            type: "string",
            description: "The due date in ISO format (should be at 9:00am in user's timezone)",
          },
          dartboard: {
            type: "string",
            description: "The title of the dartboard (project or list of tasks)",
          },
          assignees: {
            type: "array",
            items: { type: "string" },
            description: "Array of assignee names or emails (if workspace allows multiple assignees)",
          },
          assignee: {
            type: "string",
            description: "Single assignee name or email (if workspace doesn't allow multiple assignees)",
          },
          tags: {
            type: "array",
            items: { type: "string" },
            description: "Array of tags to apply to the task",
          },
          parent: {
            type: "string",
            description: "The ID of the parent task",
          },
        },
        required: ["title"],
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
      case "get_config": {
        const response = await axios.get(
          `${host}/config`,
          { headers }
        );

        const config = ConfigResponseSchema.parse(response.data);
        return {
          content: [
            { type: "text", text: JSON.stringify(config, null, 2) },
          ],
        };
      }
      case "list_tasks": {
        const params = TaskListParamsSchema.parse(request.params.arguments);
        const response = await axios.get(
          `${host}/tasks/list`,
          { headers, params },
        );

        const tasks = TaskListResponseSchema.parse(response.data);
        return {
          content: [
            { type: "text", text: JSON.stringify(tasks, null, 2) },
          ],
        };
      }
      case "create_task": {
        const taskData = TaskCreateSchema.parse(request.params.arguments);
        const wrappedData = WrappedTaskCreateSchema.parse({ item: taskData });
        
        const response = await axios.post(
          `${host}/tasks`,
          wrappedData,
          { headers }
        );

        const task = TaskSchema.parse(response.data);
        return {
          content: [
            { type: "text", text: JSON.stringify(task, null, 2) },
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
