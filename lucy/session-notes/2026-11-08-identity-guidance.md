# Lucy identity guidance note — 2026-11-08

This note records a visible conversation about establishing a user's identity across Lucy hosts, including text-only environments. It is guidance discussed in chat, not an implemented authentication system or verified identity claim.

## Proposed approach

- Use a portable credential the user controls, preferably a public/private key protected by a passkey or hardware security key.
- Enroll the public key through a trusted process. Keep the private key and recovery material out of chat and memory files.
- For authentication, have the host issue a fresh unpredictable challenge bound to the session, host and purpose. The user signs that challenge; a trusted verifier checks the signature against the enrolled key.
- Describe a successful check narrowly as proof of control of the enrolled key for that session. It does not by itself prove who is physically typing or that a shared device is being used by its rightful owner.
- A text-only host without a trusted enrollment and signature-verification path cannot reliably authenticate the user. It should state this limitation rather than infer identity from personal details, repository access or memory.
- For a user's own computer, separate OS accounts, lock sessions when unattended, and protect the credential with the account or hardware key.

Repository write access or a successful commit is evidence of access to an authorized GitHub session, not conclusive proof of the human at the keyboard. This note contains no keys, credentials, recovery phrases, or private-memory content.
