# Session relay

Publishing a session's events to the pool is **opt-in per session and off by
default**. Nothing leaves the machine that runs a session until someone asks
for it, and publication stops the moment it is stopped.

```
GET    /v1/sessions/{session}/relay            state
PUT    /v1/sessions/{session}/relay            opt in
DELETE /v1/sessions/{session}/relay            stop immediately
POST   /v1/sessions/{session}/relay/events     publish
GET    /v1/sessions/{session}/relay/events     backfill (?after=&limit=)
```

A session is a `host_executions` row owned by one of the organization's hosts.
An unknown session id is 404. Publishing while the relay is off — or after it
was stopped — is 409.

## What a relayed event may contain

`kind` is one of `started`, `status`, `output`, `tool`, `message`, `stopped`.
`payload` is a JSON **object**, at most 8 KB, nested at most three levels.
Use it for progress and small structured summaries: which step is running,
what a tool returned, a short summary of output.

## What a relayed event may never contain

The relay never carries credentials or file contents. A payload is refused at
any depth when it has a key that normalizes to any of:

`content`, `contents`, `credential`, `credentials`, `env`, `environment`,
`file`, `files`, `password`, `privatekey`, `secret`, `token`.

There is no column for a blob, no attachment field, and no way to reference a
path that the relay would read. Summaries belong in `output` payloads;
artefacts stay on the host.

## Backfill

Events carry a per-session monotonic `sequence`. A subscriber reads with
`?after=<sequence>`, which is exclusive, so:

- repeating the same read returns the same window — no duplicates;
- a client that fell behind reads forwards until a page comes back empty.

Backfill works whether or not publication is still enabled, so a subscriber can
finish reading what was published after the publisher stopped.
