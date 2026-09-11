import type { WishView } from "./api";

export function splitWishes(wishes: WishView[]): {
  coin: WishView[];
  energy: WishView[];
} {
  return {
    coin: wishes.filter((w) => w.kind === "coin"),
    energy: wishes.filter((w) => w.kind !== "coin"),
  };
}
