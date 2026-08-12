# Hyprland backend (planned)

Status: not implemented. Contributions welcome.

## Approach

Hyprland creates headless outputs at runtime:

```sh
hyprctl output create headless
```

The output appears as `HEADLESS-1` (incrementing) and is then configurable with
the usual keyword syntax:

```sh
hyprctl keyword monitor HEADLESS-1,5120x2880@60,10000x0,4
```

The trailing `4` is the scale factor, which is the part that matters: it is what
makes toolkits re-render at 4× rather than simply giving us a larger viewport.

Remove it with:

```sh
hyprctl output remove HEADLESS-1
```

## Mapping onto the trait

- `create_phantom`: `output create headless`, then read `hyprctl -j monitors`
  to find the new name.
- `move_target`: `dispatch moveworkspacetomonitor <ws> HEADLESS-1`, recording
  the origin monitor in the `RestoreToken`.
- `capture`: `grim -o HEADLESS-1`.
- `cleanup`: move the workspace back, then `output remove`. Runs under
  `PhantomGuard`; must be idempotent and must not panic.

## Note on scale

Hyprland applies fractional scaling differently from wlroots defaults, and will
refuse scale values that do not divide the resolution into whole pixels. Since
utsushot uses integer multiples of the physical mode this should not bite, but
validate it rather than assuming.
