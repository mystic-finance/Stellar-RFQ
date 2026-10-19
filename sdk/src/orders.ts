import { hash, Keypair } from "@stellar/stellar-sdk";
import { Buffer } from "buffer";

/**
 * Signs a settlement order hash the way `rfq` verifies it: ed25519 over the
 * SEP-53 digest, SHA256("Stellar Signed Message:\n" || order_hash). Get the
 * hash from the contract (`rfq.hash_rfq_order` / `hash_fixed_order`), never
 * by re-implementing it.
 */
export function signOrderHash(signer: Keypair, orderHash: Buffer | Uint8Array): { signer: Buffer; signature: Buffer } {
  if (orderHash.length !== 32) throw new Error(`order hash must be 32 bytes, got ${orderHash.length}`);
  const digest = hash(Buffer.concat([Buffer.from("Stellar Signed Message:\n", "utf8"), Buffer.from(orderHash)]));
  return { signer: Buffer.from(signer.rawPublicKey()), signature: Buffer.from(signer.sign(digest)) };
}
