# Jikji

Jikji makes local files legible to AI agents without moving or renaming the user's files.

```bash
jikji prepare ~/Documents
jikji map ~/Documents
jikji doctor ~/Documents
```

It writes `.jikji/` and `000_JIKJI_AGENT_MAP.md` with folder/file/document indexes, document text caches, and agent route guides.

Jikji is separate from Folder1004:

- **Folder1004**: GUI software for reorganizing messy Desktop/Downloads folders for people.
- **Jikji**: CLI/agent skill for non-destructive local document knowledge maps for agents.
