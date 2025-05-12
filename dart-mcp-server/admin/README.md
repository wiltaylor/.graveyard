# Admin functionality

- [Admin functionality](#admin-functionality)
  - [Local setup](#local-setup)
  - [Deploy](#deploy)

## Local setup

1. Run `yarn install` to install the dependencies
2. Run `yarn build` to build the library
3. Debug with the MCP inspector
   1. Run `yarn start:mcp-inspector`
   2. Open [the page for the inspector](http://localhost:9001?proxyPort=9000)
   3. Fill out any needed environment variables and click 'Connect'
4. To use this with Claude Desktop, add the following to your `claude_desktop_config.json`:

   ```json
   {
     "mcpServers": {
       "dart": {
         "command": "node",
         "args": ["<PATH_TO_WORKSPACE>/dart-mcp-server/dist/index.js"],
         "env": {
           "DART_TOKEN": "dsa_...",
           "DART_HOST": "http://localhost:5173"
         }
       }
     }
   }
   ```

## Deploy

1. Bump the version in `package.json`
2. Commit and push all local changes to GitHub
3. Run `npm login` if needed
4. Run `yarn release` and follow the prompts (usually they are all a yes), confirming each step by pressing enter
