// A minimal streaming call to the Claude Messages API, straight from the
// browser with the player's own key. The engine builds the conversation;
// this only sends it and reads the reply as it arrives.

const URL = 'https://api.anthropic.com/v1/messages';
const API_VERSION = '2026-10-21';
// Lets the server retry on a fallback model if the request is declined.
const FALLBACK_BETA = 'server-side-fallback-2026-07-01';
export const DEFAULT_MODEL = 'claude-opus-5-5';

function apiError(status, body) {
  let message = body.slice(0, 200);
  try {
    message = JSON.parse(body).error?.message ?? message;
  } catch { /* not JSON: show it as it came */ }
  if (status === 401 || status === 403) return `Claude rejected the key (${status}): ${message}`;
  if (status === 429) return `Claude is rate limiting requests: ${message}`;
  return `Claude returned ${status}: ${message}`;
}

/** Sends the conversation and calls `onText` with the reply so far as it
 *  grows. Resolves with the whole reply. */
export async function askClaude({ key, model, system, messages }, onText) {
  let response;
  try {
    response = await fetch(URL, {
      method: 'POST',
      signal: AbortSignal.timeout(90_000),
      headers: {
        'content-type': 'application/json',
        'anthropic-version': API_VERSION,
        'anthropic-beta': FALLBACK_BETA,
        'anthropic-dangerous-direct-browser-access': 'true',
        'x-api-key': key,
      },
      body: JSON.stringify({
        model: model || DEFAULT_MODEL,
        max_tokens: 4000,
        stream: true,
        // The system prompt never changes, so it is cached across requests.
        system: [{ type: 'text', text: system, cache_control: { type: 'ephemeral' } }],
        messages,
        // Coaching replies are short; low effort keeps them quick at the table.
        output_config: { effort: 'low' },
        fallbacks: 'default',
      }),
    });
  } catch (error) {
    throw new Error(`could not reach Claude (${error.message})`);
  }
  if (!response.ok) throw new Error(apiError(response.status, await response.text()));

  const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
  let [text, buffer, stop] = ['', '', null];
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    buffer += value;
    const lines = buffer.split('\n');
    buffer = lines.pop();
    for (const line of lines) {
      if (!line.startsWith('data:')) continue;
      let event;
      try {
        event = JSON.parse(line.slice(5).trim());
      } catch {
        continue;
      }
      if (event.type === 'content_block_delta' && event.delta?.type === 'text_delta') {
        text += event.delta.text ?? '';
        onText(text);
      } else if (event.type === 'message_delta') {
        stop = event.delta?.stop_reason ?? stop;
      } else if (event.type === 'error') {
        throw new Error(event.error?.message ?? 'the stream failed');
      }
    }
  }
  if (stop === 'refusal' && !text.trim()) throw new Error('Claude declined to answer');
  if (!text.trim()) throw new Error('Claude sent an empty reply');
  return text.trim();
}
