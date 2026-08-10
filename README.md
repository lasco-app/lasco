# Lasco

Lasco is a client-side app to back up and sync your photo library to multiple storage solutions like S3 and soon, NAS, USB drives, or any file server.

Learn more at [getlasco.app](https://getlasco.app).

## Main features

- Photo backup / storage app
- Client-side only
- End-to-end encrypted
- Multi-destination
- Conflict-free sync

## How it works

All logic runs on your device. Lasco encrypts your photos and videos locally, then pushes them to whichever remote storage you configure. Fetch from any of them to restore or get modifications made by other users of your library.

## Push safety

Push uploads each locally held `mk_*.enc` master-key file only when that file is absent from the remote; an existing remote key file is left unchanged. Media relay is opt-in: a normal push reports media missing from the local cache, while a caller can explicitly select one identity-verified, read-only source remote. Relayed encrypted blobs are staged in a unique operating-system temporary directory, decrypted before upload, and removed immediately after a successful target upload. Push never reads remote operation files; operation upload and compaction use only the target remote's local last-known state.

## Format specification

The Lasco format is fully documented: [getlasco.app/docs/format-specification/motivations](https://getlasco.app/docs/format-specification/motivations)

## License

GPL v3

## Contributing

I don't accept contributions for now.
