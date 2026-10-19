#!/usr/bin/env node
// Decode a Stellar strkey to its raw 32 bytes in hex: an account (G...) gives
// its ed25519 public key, a contract (C...) its contract hash. Zero
// dependencies — used by scripts to pass a BytesN<32> signer key, or to build
// an Address's XDR. Usage: node strkey.mjs GABC... | CABC...
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
  return Uint8Array.from(out);
}

const addr = process.argv[2];
if (!addr || !"GC".includes(addr[0])) {
  console.error("usage: node strkey.mjs G<account-address> | C<contract-address>");
  process.exit(1);
}
const decoded = base32Decode(addr); // [version(1)][key(32)][crc(2)]
const key = decoded.slice(1, 33);
process.stdout.write(Buffer.from(key).toString("hex"));
