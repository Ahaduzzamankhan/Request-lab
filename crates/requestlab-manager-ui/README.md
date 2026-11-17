# requestlab-manager-ui

The state model shared by the installer and updater windows.

`requestlab-manager` reports raw `ManagerEvent`s and never formats output. This
crate turns them into what a window draws: one stage line, a bounded log, a
download percentage with transfer rate, warnings, and a terminal
success/failure state with retry. It depends on no GUI framework, so all of the
presentation logic is unit tested on any host.

```rust
let mut flow = FlowState::new(Operation::Install);
flow.start();
flow.apply(ManagerEvent::Transfer(transfer));
assert_eq!(flow.percent(), Some(0.5));
```

The installer and updater binaries only render `FlowState`; they contain no
progress logic of their own.
