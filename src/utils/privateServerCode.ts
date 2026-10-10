/**
 * A private-server code written into the Job ID field, either as `vip:<code>`
 * or inside a pasted link (`?privateServerLinkCode=`/`linkCode=`/`code=`).
 * Returns an empty string when the field holds a plain Job ID.
 *
 * Both launch paths must agree on this: a single launch used to understand
 * pasted links while a multi launch forwarded the whole URL as the Job ID.
 */
export function parsePrivateServerCode(rawJobId: string): string {
  const raw = rawJobId.trim();
  if (!raw) return "";

  const vipPrefix = raw.match(/^vip:\s*(.+)$/i);
  if (vipPrefix?.[1]) return vipPrefix[1].trim();

  const linkLike = raw.match(/(?:privateServerLinkCode|linkCode|code)=([^&\s]+)/i);
  if (linkLike?.[1]) {
    try {
      return decodeURIComponent(linkLike[1]);
    } catch {
      return linkLike[1];
    }
  }

  return "";
}
