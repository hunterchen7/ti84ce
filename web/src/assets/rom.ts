export async function decodeRom(filename = "/sys84.bin"): Promise<Uint8Array | null> {
  try {
    const response = await fetch(filename);
    if (!response.ok) return null;

    const compressed = new Uint8Array(await response.arrayBuffer());

    // Decompress gzip
    const stream = new DecompressionStream("gzip");
    const writer = stream.writable.getWriter();
    writer.write(compressed);
    writer.close();

    const chunks: Uint8Array[] = [];
    const reader = stream.readable.getReader();
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
    }

    const totalLength = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
    const result = new Uint8Array(totalLength);
    let offset = 0;
    for (const chunk of chunks) {
      result.set(chunk, offset);
      offset += chunk.length;
    }

    return result;
  } catch {
    return null;
  }
}
