export function Player(props: { url: string; contentType?: string | null }) {
  const contentType = props.contentType?.toLowerCase() ?? "";
  const lower = props.url.toLowerCase();
  if (contentType.startsWith("video/") || /\.(mp4|webm|mov|m4v)(?:$|\?)/i.test(lower)) {
    return <video className="player" controls src={props.url} />;
  }
  if (contentType.startsWith("image/") || /\.(png|jpe?g|webp|gif|avif)(?:$|\?)/i.test(lower)) {
    return <img className="player" src={props.url} alt="" />;
  }
  if (contentType.startsWith("audio/") || /\.(mp3|m4a|aac|ogg|opus|wav|flac)(?:$|\?)/i.test(lower)) {
    return <audio className="player" controls src={props.url} />;
  }
  return null;
}
