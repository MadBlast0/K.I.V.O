/**
 * Puts React elements (key caps, links) into a translated sentence, so word order stays the
 * translator's: the message has `{name}` placeholders and each is replaced by its element.
 */
import { Fragment, type ReactNode } from "react";

export function withNodes(
  t: (key: string, values: Record<string, string>) => string,
  key: string,
  nodes: Record<string, ReactNode>,
) {
  // Placeholders become private-use markers that no translation contains.
  const names = Object.keys(nodes);
  const markers = Object.fromEntries(names.map((name, i) => [name, String.fromCharCode(0xe000 + i)]));
  const text = t(key, markers);
  const split = new RegExp(`([${names.map((n) => markers[n]).join("")}])`);
  return text.split(split).map((part, i) => {
    const name = names.find((n) => markers[n] === part);
    return <Fragment key={i}>{name ? nodes[name] : part}</Fragment>;
  });
}
