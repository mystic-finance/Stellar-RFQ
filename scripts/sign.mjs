#!/usr/bin/env node
// Sign a settlement order hash the way the contract verifies it: the ed25519
// signature is over SHA256("Stellar Signed Message:\n" || order_hash), the
// SEP-53 digest. Zero dependencies.
//
//   node sign.mjs <S-secret-seed> <order-hash-hex>   -> {"signer":"..","signature":".."}
//
// Print only one field with --signer / --signature.
import { createHash, createPrivateKey, sign as edSign, createPublicKey } from "node:crypto";

const B32 = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

function base32Decode(s) {
  let bits = 0, value = 0;
  const out = [];
  for (const c of s.replace(/=+$/, "")) {
    const idx = B32.indexOf(c);
    if (idx === -1) throw new Error(`invalid base32 char: ${c}`);
    value = (value << 5) | idx;
    bits += 5;
    if (bits >= 8) {
      bits -= 8;
      out.push((value >>> bits) & 0xff);
    }
  }
  return Buffer.from(out);
}

// PKCS#8 wrapper for a raw Ed25519 seed, so node's crypto will load it.
const PKCS8_PREFIX = Buffer.from("302e020100300506032b657004220420", "hex");
function privateKeyFromSeed(seed) {
  return createPrivateKey({
    key: Buffer.concat([PKCS8_PREFIX, seed]),
    format: "der",
    type: "pkcs8",
  });
}

const args = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const flags = new Set(process.argv.slice(2).filter((a) => a.startsWith("--")));
const [secret, orderHashHex] = args;

if (!secret || secret[0] !== "S" || !orderHashHex) {
  console.error("usage: node sign.mjs <S-secret-seed> <order-hash-hex> [--signer|--signature]");
  process.exit(1);
}

const orderHash = Buffer.from(orderHashHex.replace(/^0x/, "").replace(/"/g, ""), "hex");
if (orderHash.length !== 32) {
  console.error(`order hash must be 32 bytes, got ${orderHash.length}`);
  process.exit(1);
}

// [version(1)][seed(32)][crc(2)]
const seed = base32Decode(secret).subarray(1, 33);
const key = privateKeyFromSeed(seed);

const digest = createHash("sha256")
  .update(Buffer.concat([Buffer.from("Stellar Signed Message:\n", "utf8"), orderHash]))
  .digest();

const signature = edSign(null, digest, key).toString("hex");
const signer = createPublicKey(key)
  .export({ format: "der", type: "spki" })
  .subarray(-32)
  .toString("hex");

if (flags.has("--signer")) process.stdout.write(signer);
else if (flags.has("--signature")) process.stdout.write(signature);
else process.stdout.write(JSON.stringify({ signer, signature }));
