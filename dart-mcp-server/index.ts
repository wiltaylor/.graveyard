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

// Task schemas
const AssigneeSchema = z.object({
  name: z.string(),
  email: z.string(),
});

const TaskCreateSchema = z.object({
  title: z.string(),
  description: z.string().optional(),
  dartboard: z.string().optional(),
  status: z.string().optional(),
  priority: z.string().nullable().optional(),
  size: z.number().nullable().optional(),
  start_at: z.string().nullable().optional(),
  due_at: z.string().nullable().optional(),
  assignees: z.array(z.string()).optional(),
  assignee: z.string().optional(),
  tags: z.array(z.string()).optional(),
  parent: z.string().nullable().optional(),
});

const TaskUpdateSchema = TaskCreateSchema.extend({
  id: z.string(),
});

const TaskSchema = TaskUpdateSchema.extend({
  permalink: z.string(),
  description: z.string(),
  dartboard: z.string(),
  status: z.string(),
});

const WrappedTaskCreateSchema = z.object({
  item: TaskCreateSchema,
});

const WrappedTaskUpdateSchema = z.object({
  item: TaskUpdateSchema,
});

const WrappedTaskSchema = z.object({
  item: TaskSchema,
});

const TaskIdSchema = z.object({
  id: z.string().regex(/^[a-zA-Z0-9]{12}$/, "Task ID must be 12 alphanumeric characters"),
});

// Doc schemas
const DocCreateSchema = z.object({
  title: z.string(),
  text: z.string().optional(),
  folder: z.string().optional(),
});

const DocUpdateSchema = DocCreateSchema.extend({
  id: z.string(),
});

const DocSchema = DocUpdateSchema.extend({
  permalink: z.string(),
  folder: z.string(),
});

const WrappedDocCreateSchema = z.object({
  item: DocCreateSchema,
});

const WrappedDocUpdateSchema = z.object({
  item: DocUpdateSchema,
});

const WrappedDocSchema = z.object({
  item: DocSchema,
});

const DocIdSchema = z.object({
  id: z.string().regex(/^[a-zA-Z0-9]{12}$/, "Doc ID must be 12 alphanumeric characters"),
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

const DocListParamsSchema = z.object({
  folder: z.string().optional(),
  folder_duid: z.string().optional(),
  duids: z.string().optional(),
  in_trash: z.boolean().optional(),
  is_draft: z.boolean().optional(),
  limit: z.number().optional(),
  offset: z.number().optional(),
  s: z.string().optional(),
  text: z.string().optional(),
  title: z.string().optional(),
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

const PaginationResponseSchema = z.object({
  count: z.number(),
  next: z.string().nullable(),
  previous: z.string().nullable(),
});

const TaskListResponseSchema = PaginationResponseSchema.extend({
  results: z.array(TaskSchema),
});

const DocListResponseSchema = PaginationResponseSchema.extend({
  results: z.array(DocSchema),
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
    {
      name: "get_task",
      description: "Retrieve an existing task by its ID. Returns the task's information including title, description, status, priority, dates, and more.",
      inputSchema: {
        type: "object",
        properties: {
          id: {
            type: "string",
            description: "The 12-character alphanumeric ID of the task",
            pattern: "^[a-zA-Z0-9]{12}$",
          },
        },
        required: ["id"],
      },
    },
    {
      name: "update_task",
      description: "Update an existing task. You can modify any of its properties including title, description, status, priority, dates, assignees, and more.",
      inputSchema: {
        type: "object",
        properties: {
          id: {
            type: "string",
            description: "The 12-character alphanumeric ID of the task",
            pattern: "^[a-zA-Z0-9]{12}$",
          },
          title: {
            type: "string",
            description: "The title of the task",
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
        required: ["id"],
      },
    },
    {
      name: "delete_task",
      description: "Move an existing task to the trash, where it can be recovered if needed. Nothing else about the task will be changed.",
      inputSchema: {
        type: "object",
        properties: {
          id: {
            type: "string",
            description: "The 12-character alphanumeric ID of the task",
            pattern: "^[a-zA-Z0-9]{12}$",
          },
        },
        required: ["id"],
      },
    },
    {
      name: "list_docs",
      description: "List docs from Dart with optional filtering parameters. You can filter by folder, title, text content, and more.",
      inputSchema: {
        type: "object",
        properties: {
          folder: {
            type: "string",
            description: "Filter by folder title",
          },
          folder_duid: {
            type: "string",
            description: "Filter by folder DUID",
          },
          duids: {
            type: "string",
            description: "Filter by DUIDs",
          },
          in_trash: {
            type: "boolean",
            description: "Filter by trash status",
          },
          is_draft: {
            type: "boolean",
            description: "Filter by draft status",
          },
          limit: {
            type: "number",
            description: "Number of results per page",
          },
          offset: {
            type: "number",
            description: "Initial index for pagination",
          },
          s: {
            type: "string",
            description: "Search by title, text, or folder title",
          },
          text: {
            type: "string",
            description: "Filter by text content",
          },
          title: {
            type: "string",
            description: "Filter by title",
          },
        },
        required: [],
      },
    },
    {
      name: "create_doc",
      description: "Create a new doc in Dart. You can specify title, text content, and folder.",
      inputSchema: {
        type: "object",
        properties: {
          title: {
            type: "string",
            description: "The title of the doc (required)",
          },
          text: {
            type: "string",
            description: "The text content of the doc, which can include markdown formatting",
          },
          folder: {
            type: "string",
            description: "The title of the folder to place the doc in",
          },
        },
        required: ["title"],
      },
    },
    {
      name: "get_doc",
      description: "Retrieve an existing doc by its ID. Returns the doc's information including title, text content, folder, and more.",
      inputSchema: {
        type: "object",
        properties: {
          id: {
            type: "string",
            description: "The 12-character alphanumeric ID of the doc",
            pattern: "^[a-zA-Z0-9]{12}$",
          },
        },
        required: ["id"],
      },
    },
    {
      name: "update_doc",
      description: "Update an existing doc. You can modify its title, text content, and folder.",
      inputSchema: {
        type: "object",
        properties: {
          id: {
            type: "string",
            description: "The 12-character alphanumeric ID of the doc",
            pattern: "^[a-zA-Z0-9]{12}$",
          },
          title: {
            type: "string",
            description: "The title of the doc",
          },
          text: {
            type: "string",
            description: "The text content of the doc, which can include markdown formatting",
          },
          folder: {
            type: "string",
            description: "The title of the folder to place the doc in",
          },
        },
        required: ["id"],
      },
    },
    {
      name: "delete_doc",
      description: "Move an existing doc to the trash, where it can be recovered if needed. Nothing else about the doc will be changed.",
      inputSchema: {
        type: "object",
        properties: {
          id: {
            type: "string",
            description: "The 12-character alphanumeric ID of the doc",
            pattern: "^[a-zA-Z0-9]{12}$",
          },
        },
        required: ["id"],
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

        const task = WrappedTaskSchema.parse(response.data);
        return {
          content: [
            { type: "text", text: JSON.stringify(task, null, 2) },
          ],
        };
      }
      case "get_task": {
        const { id } = TaskIdSchema.parse(request.params.arguments);
        const response = await axios.get(
          `${host}/tasks/${id}`,
          { headers }
        );

        const task = WrappedTaskSchema.parse(response.data);
        return {
          content: [
            { type: "text", text: JSON.stringify(task, null, 2) },
          ],
        };
      }
      case "update_task": {
        const taskData = TaskIdSchema.merge(TaskUpdateSchema).parse(request.params.arguments);
        const { id } = taskData;
        const wrappedData = WrappedTaskUpdateSchema.parse({ item: taskData });
        const response = await axios.put(
          `${host}/tasks/${id}`,
          wrappedData,
          { headers }
        );

        const task = WrappedTaskSchema.parse(response.data);
        return {
          content: [
            { type: "text", text: JSON.stringify(task, null, 2) },
          ],
        };
      }
      case "delete_task": {
        const { id } = TaskIdSchema.parse(request.params.arguments);
        const response = await axios.delete(
          `${host}/tasks/${id}`,
          { headers }
        );

        const task = WrappedTaskSchema.parse(response.data);
        return {
          content: [
            { type: "text", text: JSON.stringify(task, null, 2) },
          ],
        };
      }
      case "list_docs": {
        const params = DocListParamsSchema.parse(request.params.arguments);
        const response = await axios.get(
          `${host}/docs/list`,
          { headers, params }
        );

        const docs = DocListResponseSchema.parse(response.data);
        return {
          content: [
            { type: "text", text: JSON.stringify(docs, null, 2) },
          ],
        };
      }
      case "create_doc": {
        const docData = DocCreateSchema.parse(request.params.arguments);
        const wrappedData = WrappedDocCreateSchema.parse({ item: docData });
        const response = await axios.post(
          `${host}/docs`,
          wrappedData,
          { headers }
        );

        const doc = WrappedDocSchema.parse(response.data);
        return {
          content: [
            { type: "text", text: JSON.stringify(doc, null, 2) },
          ],
        };
      }
      case "get_doc": {
        const { id } = DocIdSchema.parse(request.params.arguments);
        const response = await axios.get(
          `${host}/docs/${id}`,
          { headers }
        );

        const doc = WrappedDocSchema.parse(response.data);
        return {
          content: [
            { type: "text", text: JSON.stringify(doc, null, 2) },
          ],
        };
      }
      case "update_doc": {
        const docData = DocIdSchema.merge(DocUpdateSchema).parse(request.params.arguments);
        const { id } = docData;
        const wrappedData = WrappedDocUpdateSchema.parse({ item: docData });
        const response = await axios.put(
          `${host}/docs/${id}`,
          wrappedData,
          { headers }
        );

        const doc = WrappedDocSchema.parse(response.data);
        return {
          content: [
            { type: "text", text: JSON.stringify(doc, null, 2) },
          ],
        };
      }
      case "delete_doc": {
        const { id } = DocIdSchema.parse(request.params.arguments);
        const response = await axios.delete(
          `${host}/docs/${id}`,
          { headers }
        );

        const doc = WrappedDocSchema.parse(response.data);
        return {
          content: [
            { type: "text", text: JSON.stringify(doc, null, 2) },
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
