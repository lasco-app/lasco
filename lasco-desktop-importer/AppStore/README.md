# Mac App Store signing material

This folder contains the committed entitlements required by the Compose/JVM application.
Provisioning profiles must never be committed. Download the two **Mac App Store** profiles from
Apple Developer and place them here with exactly these names:

- `embedded.provisionprofile` for `app.lasco.desktopimporter`
- `runtime.provisionprofile` for `com.oracle.java.app.lasco.desktopimporter`

Both profiles use the Mac App Distribution certificate. The bundle ID and team ID in
`entitlements.plist` must match the profile. The JDK runtime requires its own profile and its own
entitlements because it is signed separately inside the app bundle.

Build from Xcode with the `LascoImporter-AppStore` scheme or run:

```sh
scripts/package-app-store.sh
```

The output `.pkg` belongs in App Store Connect via Transporter. This is a Mac App Store package,
so it must not be notarized.
