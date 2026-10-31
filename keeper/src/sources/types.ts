import type { Nav } from "../convert.js";

/** Somewhere an issuer publishes NAV: a contract, an API, a file. */
export interface NavSource {
  name: string;
  read(): Promise<Nav>;
}
