# iPhone/iPad backups (Finder/iTunes): what BH World Tool reads

`crates/bhw-ios` reads The Blockheads worlds from local iOS backups. It only reads; it never changes a backup and
never talks to a device. This page describes the parts of the format it uses. The description is put together from
public sources (listed at the end); the code is our own.

## Where backups are

| OS | Folder |
| --- | --- |
| Windows (iTunes from apple.com) | `%APPDATA%\Apple Computer\MobileSync\Backup\` |
| Windows (iTunes / Apple Devices from the Microsoft Store) | `%USERPROFILE%\Apple\MobileSync\Backup\` |
| macOS (Finder or iTunes) | `~/Library/Application Support/MobileSync/Backup/` (needs Full Disk Access) |
| Linux | none; the user picks a folder (e.g. a backup copied from a Mac/PC, or one made with libimobiledevice) |

Each backup is one subdirectory, named after the device UDID (archived copies get a date suffix).

## Descriptive plists

All may be binary or XML plists.

- `Info.plist`: `Device Name`, `Product Type` (e.g. `iPhone14,2`), `Product Version` (iOS version),
  `Last Backup Date`, `Unique Identifier`, list of installed apps.
- `Manifest.plist`: `IsEncrypted` (bool), `Version`, `Date`, `Lockdown` (dict with `DeviceName`, `ProductType`,
  `ProductVersion`, `UniqueDeviceID`), and for encrypted backups `BackupKeyBag` (data) and, from iOS 10.2,
  `ManifestKey` (data).
- `Status.plist`: `Date`, `IsFullBackup`, `SnapshotState`, `Version`.

`read_backup_info` takes the device name, model and iOS version from `Info.plist`, falling back to `Lockdown`; the
date from `Info.plist`, then `Status.plist`, then `Manifest.plist`. A folder without `Manifest.plist` is not a backup.

## Manifest.db and stored files (iOS 10+)

`Manifest.db` is an SQLite database. Table `Files`:

| column | meaning |
| --- | --- |
| `fileID` | `sha1(domain + "-" + relativePath)`, lowercase hex |
| `domain` | e.g. `HomeDomain`, `AppDomain-<bundle id>`, `AppDomainGroup-...` |
| `relativePath` | path inside the domain's container |
| `flags` | 1 file, 2 directory, 4 symlink |
| `file` | `NSKeyedArchiver` binary plist of an `MBFile` |

The file contents are stored at `<backup>/<first two hex chars of fileID>/<fileID>` (older backups: `<backup>/
<fileID>`). `fileID` is validated as hex before it is used as a path.

`MBFile` (resolved through `$top.root` and `$objects` UIDs) has, among others, `Size`, `LastModified`, `Birth`,
`Mode`, `ProtectionClass`, `RelativePath` and, in encrypted backups, `EncryptionKey`: a UID to an `NSData`
(`NS.data`) holding a 4-byte little-endian protection class followed by the 40-byte RFC 3394 wrapped file key.

iOS 9 and older use `Manifest.mbdb` instead of `Manifest.db`; this is reported as unsupported.

## Encrypted backups

1. **Keybag** (`BackupKeyBag`): TLV records `tag[4] length[4, big-endian] value`. The first `UUID` belongs to the
   keybag itself; header tags include `VERS`, `TYPE`, `WRAP`, `SALT`, `ITER` and from iOS 10.2 `DPWT`, `DPIC`, `DPSL`.
   Each later `UUID` starts a class key record with `CLAS` (protection class), `WRAP` (bit 2 = wrapped with the
   password key), `KTYP` and `WPKY` (wrapped key). Integers are 4-byte big-endian.
2. **Password key**: iOS 10.2+: `k1 = PBKDF2-HMAC-SHA256(password, DPSL, DPIC, 32)` (DPIC is typically 10,000,000),
   then `key = PBKDF2-HMAC-SHA1(k1, SALT, ITER, 32)`. Older keybags (no `DPSL`): only the SHA1 step with the password.
3. **Class keys**: `WPKY` unwrapped with `key` using AES key unwrap (RFC 3394). A failing integrity check means the
   password is wrong (`Error::WrongPassword`).
4. **Manifest.db**: `ManifestKey` = 4-byte little-endian class + wrapped key; unwrap with that class key, then
   AES-256-CBC with a zero IV. The plain text may end with PKCS#7 padding; it is dropped when doing so leaves a whole
   number of 512-byte pages. Backups from iOS 10.0/10.1 have no `ManifestKey` and an unencrypted `Manifest.db`.
   The decrypted database is opened in memory (never written to disk).
5. **Files**: per-file key from `MBFile.EncryptionKey` (skip the 4-byte class prefix), unwrapped with the class key
   of `MBFile.ProtectionClass`; contents are AES-256-CBC with a zero IV, trimmed to `MBFile.Size`.

## The Blockheads in a backup

The game's container is `AppDomain-<bundle id>`. The bundle id is not hard-coded: `IosBackup::blockheads_apps` lists
every `AppDomain-*` that has `Library/Application Support/saves/<32 lowercase hex>/world_db/data.mdb`. From that
container `bhw-ios` takes:

- `Library/Application Support/saves/<id>/**` for every world id with the `world_db/data.mdb` marker, as
  `bhw_world::WorldFiles` (unencrypted backups: `Payload::File` pointing into the backup; encrypted: decrypted
  `Payload::Bytes`);
- `Library/Application Support/game_db/**` (the game's world list), as `WorldFile`s relative to `game_db/`.

The iOS game is the native original, so `data.mdb` is LMDB in the 64-bit layout on 64-bit devices and the 32-bit
layout on old 32-bit ones; `bhw-ios` only extracts bytes, `bhw-lmdb`/`bhw-save` detect the layout.

## Verification status

No real iOS backup was available while writing this. Everything is tested against synthetic backups built by the
tests (SQLite via rusqlite, plists via the `plist` crate, keybags/wrapping/encryption with this crate's code, which is
checked against RFC 3394 §4.3/§4.6 and NIST SP 800-38A F.2.5/F.2.6 vectors, and a known public `fileID`). The test
`crates/bhw-ios/tests/real_backup.rs` runs on a real backup when `BHW_IOS_BACKUP` (and `BHW_IOS_PASSWORD` for
encrypted ones) is set.

## Public sources

- *iphone-dataprotection* (Jean-Baptiste Bédrune, Jean Sigwald; "iPhone data protection in depth", HITB 2011):
  keybag TLV layout, class keys, backup decryption.
- *Mobile Verification Toolkit (mvt)* by Amnesty International Security Lab: `Manifest.db` layout, decryption of
  iOS 10.2+ backups.
- *iOSbackup* (Python, Avi Alkalay) and *iphone_backup_decrypt* (Python, jsharkey13): `ManifestKey`, `MBFile`
  `EncryptionKey`, DPSL/DPIC derivation.
- Stack Overflow, "How to decrypt an encrypted Apple iTunes iPhone backup?" (answer by andrewdotn): end-to-end
  description of the iOS 10.2+ scheme.
- Apple Support articles "Locate backups of your iPhone, iPad and iPod touch" for the folder locations.
- RFC 3394 (AES Key Wrap), NIST SP 800-38A (CBC), RFC 8018 (PBKDF2).
