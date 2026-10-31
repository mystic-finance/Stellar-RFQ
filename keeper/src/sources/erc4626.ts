import type { Nav } from "../convert.js";
import type { NavSource } from "./types.js";

/**
 * NAV per share from an ERC-4626 / ERC-7540 vault on an EVM chain, e.g.
 * Centrifuge's JAAA/USDC vault on Ethereum: `convertToAssets(1 share)`, in the
 * vault asset's decimals. Uses plain JSON-RPC `eth_call`, no EVM library.
 */
export function erc4626Source(opts: { name: string; rpcUrl: string; vault: string }): NavSource {
  const call = async (to: string, data: string): Promise<bigint> => {
    const res = await fetch(opts.rpcUrl, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "eth_call", params: [{ to, data }, "latest"] }),
    });
    const body = (await res.json()) as { result?: string; error?: { message: string } };
    if (!body.result || body.result === "0x") throw new Error(`eth_call ${data.slice(0, 10)} on ${to}: ${body.error?.message ?? "empty"}`);
    return BigInt(body.result);
  };
  const address = (word: bigint) => "0x" + word.toString(16).padStart(40, "0").slice(-40);
  const SHARE = "0xa8d5fd65"; // share()
  const ASSET = "0x38d52e0f"; // asset()
  const DECIMALS = "0x313ce567"; // decimals()
  const CONVERT = "0x07a2d13a"; // convertToAssets(uint256)

  return {
    name: opts.name,
    async read(): Promise<Nav> {
      // ERC-7540 vaults expose the share token separately; plain ERC-4626 vaults are their own share.
      const share = await call(opts.vault, SHARE).then(address).catch(() => opts.vault);
      const asset = address(await call(opts.vault, ASSET));
      const [shareDecimals, assetDecimals] = await Promise.all([call(share, DECIMALS), call(asset, DECIMALS)]);
      const oneShare = (10n ** shareDecimals).toString(16).padStart(64, "0");
      const value = await call(opts.vault, CONVERT + oneShare);
      return { value, decimals: Number(assetDecimals), timestamp: Math.floor(Date.now() / 1000) };
    },
  };
}
