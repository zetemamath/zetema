// Burning the token for hours of search, from the page: the visitor's own wallet signs an approve and a sponsor call
// to HuntSponsor on Robinhood Chain. Nothing passes through the lab; it only reads the Sponsored event afterwards.

import type { Chain } from "./lab";

interface Wallet {
  request(args: { method: string; params?: unknown[] }): Promise<unknown>;
}

const ROBINHOOD = {
  chainId: "0x1237", // 4663
  chainName: "Robinhood Chain",
  nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
  rpcUrls: ["https://rpc.mainnet.chain.robinhood.com"],
  blockExplorerUrls: ["https://robinhoodchain.blockscout.com"],
};
// cast sig: balanceOf(address), allowance(address,address), approve(address,uint256), sponsor(uint16,uint8,uint8,uint256)
const BALANCE = "0x70a08231", ALLOWANCE = "0xdd62ed3e", APPROVE = "0x095ea7b3", SPONSOR = "0x88bd6b7d";

const word = (x: bigint | number | string) =>
  (typeof x === "string" ? x.toLowerCase().replace(/^0x/, "") : BigInt(x).toString(16)).padStart(64, "0");
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Whole tokens, from wei, with thousands separators. */
export const tokens = (wei: bigint) => (wei / 10n ** 18n).toLocaleString("en");

function wallet(): Wallet {
  const w = (window as unknown as { ethereum?: Wallet }).ethereum;
  if (!w) throw new Error("no wallet found in this browser: open the page in a wallet's browser, or install one");
  return w;
}

async function receipt(w: Wallet, hash: string, say: (s: string) => void): Promise<void> {
  for (let i = 0; i < 240; i++) {
    const r = (await w.request({ method: "eth_getTransactionReceipt", params: [hash] })) as { status: string } | null;
    if (r) {
      if (r.status !== "0x1") throw new Error("the transaction failed on chain");
      return;
    }
    say(`waiting for the chain${".".repeat((i % 3) + 1)}`);
    await wait(1500);
  }
  throw new Error("no receipt after six minutes; check the wallet's activity");
}

/** Burns hours * unit tokens for hours of search on C(v, k, t); returns the sponsor transaction's hash. */
export async function sponsorSearch(chain: Chain, v: number, k: number, t: number, hours: number, say: (s: string) => void): Promise<string> {
  if (!chain.sponsor || !chain.token || !chain.unit) throw new Error("the sponsor contract is not live yet");
  const w = wallet();
  say("connecting the wallet");
  const [from] = (await w.request({ method: "eth_requestAccounts" })) as string[];
  try {
    await w.request({ method: "wallet_switchEthereumChain", params: [{ chainId: ROBINHOOD.chainId }] });
  } catch (error) {
    if ((error as { code?: number }).code !== 4902) throw error;
    await w.request({ method: "wallet_addEthereumChain", params: [ROBINHOOD] });
  }
  const amount = BigInt(chain.unit) * BigInt(hours);
  const read = async (data: string) => BigInt((await w.request({ method: "eth_call", params: [{ to: chain.token, data }, "latest"] })) as string);
  const balance = await read(BALANCE + word(from));
  if (balance < amount) throw new Error(`this wallet holds ${tokens(balance)} $${chain.ticker ?? "ZETEMA"}; ${hours} h burns ${tokens(amount)}`);
  if ((await read(ALLOWANCE + word(from) + word(chain.sponsor))) < amount) {
    say("approve the amount in your wallet (1 of 2)");
    const approve = (await w.request({ method: "eth_sendTransaction", params: [{ from, to: chain.token, data: APPROVE + word(chain.sponsor) + word(amount) }] })) as string;
    await receipt(w, approve, say);
  }
  say("confirm the burn in your wallet (2 of 2)");
  const hash = (await w.request({ method: "eth_sendTransaction", params: [{ from, to: chain.sponsor, data: SPONSOR + word(v) + word(k) + word(t) + word(hours) }] })) as string;
  await receipt(w, hash, say);
  return hash;
}
