# Building and releasing rocketviz

## What's in the app
The installers bundle:
- the app
- the OpenRocket engine folder: `Resources/engines/openrocket/`
- a trimmed Java runtime, so users don't need Java: `Resources/jre/`

Both bundled parts come from `engines/openrocket`:
```
cd engines/openrocket && ./gradlew bundle   # build/engine/ and build/jre/ (for this OS)
npx --prefix web tauri build                # installers in target/release/bundle/
```
Sizes on macOS:

| Part | Size |
|---|---|
| App | 146 MB |
| Engine (OpenRocket JAR) | 78 MB |
| Java runtime | 47 MB |

`./gradlew bundle test` also runs `BundleParityTest`. It requires the bundled runtime and JAR to simulate OpenRocket's example designs exactly as a full JDK does.

## Releasing
1. Set the new version in `src-tauri/tauri.conf.json`. Also set it in `Cargo.toml` (`[workspace.package]`) and `web/package.json` so they agree.
2. Commit, then tag and push:
   ```
   git tag v0.2.0 && git push origin v0.2.0
   ```
3. The `release` workflow builds macOS, Windows and Linux. It uploads the installers, the update packages, `latest.json` and `SHA256SUMS.txt` to a **draft** release.
4. Check the draft and try an installer.
5. **Publish** the draft. Installed apps only see an update once the release is published, not while it's a draft or pre-release. They check at startup, and the user can also choose **File → Check for updates…**.

## Secrets
Each part of signing is skipped, with a warning in the log, until its secrets exist. Set them with `gh secret set NAME`, or under the repository's Settings → Secrets and variables → Actions.

### Update signing (needed for the updater)
The key pair was generated with `tauri signer generate`. The public key is in `tauri.conf.json` (`plugins.updater.pubkey`), and the private key is in `~/.tauri/rocketviz-updater.key`.
```
gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/rocketviz-updater.key
```
The key has no password, so `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` stays unset.

**Back up the private key**, for example in a password manager. Installed apps accept only updates signed with it. If it's lost, existing users must reinstall by hand to get a new key.

### macOS signing and notarization
1. **Certificate:** create a *Developer ID Application* certificate (Xcode → Settings → Accounts → Manage Certificates, or developer.apple.com), and export it from Keychain Access as a `.p12` file with a password.
2. **Signing secrets:**
   ```
   base64 -i DeveloperID.p12 | gh secret set APPLE_CERTIFICATE
   gh secret set APPLE_CERTIFICATE_PASSWORD               # the .p12 password
   gh secret set APPLE_SIGNING_IDENTITY                   # "Developer ID Application: Your Name (TEAMID)"
   ```
3. **Notarization:** create an app-specific password at appleid.apple.com (Sign-In and Security → App-Specific Passwords), then:
   ```
   gh secret set APPLE_ID                                 # your Apple ID email
   gh secret set APPLE_PASSWORD                           # the app-specific password
   gh secret set APPLE_TEAM_ID
   ```

The workflow signs every binary in the Java runtime, using the hardened runtime and the JIT entitlements in `src-tauri/macos/java.entitlements`, with `scripts/macos-sign-runtime.sh`. Tauri then signs and notarizes the app. The Java runtime must be signed first because Tauri copies resources without signing them.

### Windows
Not signed yet. SignPath Foundation or Azure Trusted Signing comes before 1.0 (see architecture decision 5).

## Unsigned builds (early testers)
- **macOS:** unsigned or un-notarized downloads are blocked by Gatekeeper. After copying the app to Applications, run:
  ```
  xattr -dr com.apple.quarantine /Applications/rocketviz.app
  ```
- **Windows:** SmartScreen warns about an unknown publisher. Choose **More info → Run anyway**.
- **Linux:** run the AppImage after `chmod +x`, or install the `.deb`. Check downloads against `SHA256SUMS.txt`.

## Testing an installed build
`ROCKETVIZ_SMOKE=1` runs the built-in self-test (see `docs/tauri-spike.md`). Two more variables extend it:
- `ROCKETVIZ_SMOKE_DESIGN=/path/to/rocket.ork` also simulates a design.
- `ROCKETVIZ_SMOKE_UPDATE=1` also checks for an update, downloads it, verifies it and installs it.

The updater was verified locally this way (v0.1.0 updating itself to v0.1.1). A tampered update package was rejected ("signature verification failed").
