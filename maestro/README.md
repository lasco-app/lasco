# Visual flows

`expert-local-fs` is one shared Maestro flow for the iOS and Android apps. It
inspects onboarding's fresh/existing branches, enables Expert Mode, creates a
library with a local filesystem remote, imports existing device media, and
captures the Home, Albums, Status, and Manage tabs.

The simulator and emulator must already contain at least one image. The runner
grants the app media permission, but it never inserts fixture media.

Build and install each app on its target device, then capture both sides:

```bash
pnpm flow:capture -- --flow expert-local-fs \
  --ios-udid <ios-simulator-udid> \
  --android-udid <android-emulator-serial>
```

For a single-platform diagnostic run, add `--platform ios` or
`--platform android`. Each run gets an immutable directory under
`maestro/artifacts/runs/`. The runner normalizes named Maestro screenshots to
`ios/screenshots/` and `android/screenshots/`, while retaining Maestro's raw
logs and command artifacts below each platform directory.

To add the other platform to an existing single-platform capture, append it to
that run instead of creating a separate viewer entry:

```bash
pnpm flow:capture -- --flow expert-local-fs --platform android \
  --android-udid <android-emulator-serial> --append-run <run-id>
```

Open a completed capture in the React Flow viewer with the run ID printed by
the capture command:

```bash
pnpm flow:view -- --run <run-id>
```

The viewer intentionally lays out every screen in fixed iOS and Android
columns. Vertical arrows show flow navigation; dashed horizontal links pair the
same logical screen across platforms.
