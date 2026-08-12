# sway backend (planned)

Status: not implemented. Contributions welcome; this is the most tractable
backend in the project.

## Why it should be easier than niri

sway creates headless outputs at runtime, which is exactly the primitive niri
lacks:

```sh
swaymsg create_output
```

The new output is named `HEADLESS-1` (incrementing), and can then be configured
like any other:

```sh
swaymsg output HEADLESS-1 mode 5120x2880 scale 4
swaymsg output HEADLESS-1 position 10000 0
```

Placing it far outside the physical layout keeps it from disturbing the visible
arrangement. Destroy it afterwards with:

```sh
swaymsg output HEADLESS-1 unplug
```

## Mapping onto the trait

- `create_phantom`: `create_output`, then diff `get_outputs` before and after
  to learn the new name, since `create_output` does not return it.
- `move_target`: `move workspace to output HEADLESS-1`, recording the origin
  output in the `RestoreToken`.
- `capture`: `grim -o HEADLESS-1`, same as niri.
- `cleanup`: move the workspace back, then `unplug`. Runs under `PhantomGuard`,
  so it must be idempotent and must not panic.

## Talking to sway

Either shell out to `swaymsg --raw --type get_outputs` and parse the JSON with
the `serde_json` already in the tree, or add the `swayipc` crate. Shelling out
keeps the dependency tree lean and matches how the niri backend starts out;
prefer it unless the IPC volume makes it awkward.
