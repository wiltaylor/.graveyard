# Manual Test: Worker Restart Core

Use this recipe to verify that a worker with a broken AI-core resume path can be replaced by a fresh worker seeded from the prior transcript.

## Parent-Gated Restart

1. Start the aciddog daemon with a config that declares a local worker template.
2. From a parent agent pane, create a worker:

   ```sh
   aciddog worker create --template <some-local-template> --task "<some-brief>"
   ```

3. Note the reply. It has the worker display id and session UUID:

   ```text
   <template>#abc12345 00000000-0000-0000-0000-000000000000
   ```

4. Let the worker do enough work to create a useful transcript.
5. Locate the worker session JSONL. Agent and worker transcripts are written under:

   ```text
   <daemon-cwd>/.aciddog/agent-chat/<sanitized-worker-display-id>/<session-hash>.jsonl
   ```

   The display id is sanitized for the path; for example `mycoder#abc12345` becomes `mycoder_abc12345`. The session hash is shown in the worker pane header and in the `AgentInfo` block.

6. Corrupt the JSONL so `claude --resume` fails. For example:

   ```sh
   printf '\n{\"bad\":' >> <daemon-cwd>/.aciddog/agent-chat/mycoder_abc12345/<session-hash>.jsonl
   ```

   If you delete the JSONL instead, you can still test restart behavior, but you cannot verify retention of the original file. Prefer corrupting in place for the forensics check.

7. From the parent pane, run:

   ```sh
   aciddog worker restart-core --id mycoder#abc12345
   ```

8. Observe that a new worker pane appears. Its first prompt should contain:

   - `Your prior AI-core session was lost`
   - `## Transcript`
   - `## Original brief`
   - the original task brief

9. Verify the broken pane shows a system info crumb like:

   ```text
   [restart-core] replaced by <new-display-id>; old session <old-session-uuid> JSONL retained for forensics
   ```

10. Verify the new pane shows a system info crumb like:

    ```text
    [restart-core] resumed from <old-display-id>; transcript contains <N> blocks (<M> chars after cap)
    ```

11. Verify the original JSONL is still on disk:

    ```sh
    test -f <daemon-cwd>/.aciddog/agent-chat/mycoder_abc12345/<session-hash>.jsonl
    ```

12. Continue the task in the new worker and confirm it can recover context from the preamble, transcript, and original brief.

## Supervisor Force Variant

From a supervisor-capable agent pane, use the force variant when the parent-gated path is not available:

```sh
aciddog worker force-restart-core --id mycoder#abc12345
```

To switch AI cores for the replacement worker:

```sh
aciddog worker force-restart-core --id mycoder#abc12345 --ai-core <core-id>
```

The force path bypasses the parent check. The replacement still reuses the broken worker's template, original parent, cwd, and properties.
