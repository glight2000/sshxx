/**
 * @file Encryption of byte streams based on a random key.
 *
 * This is used for end-to-end encryption between the terminal source and its
 * client. Keep this file consistent with the Rust implementation.
 */

const SALT: string =
  "This is a non-random salt for sshx.io, since we want to stretch the security of 83-bit keys!";

export class Encrypt {
  private aesKey: CryptoKey;
  private checkpointKey: CryptoKey;
  private outputMaterial?: CryptoKey;
  private outputKeys = new Map<
    number,
    { epoch: string; key: Promise<CryptoKey> }
  >();
  private constructor(
    aesKey: CryptoKey,
    checkpointKey: CryptoKey,
    outputMaterial?: CryptoKey,
  ) {
    this.aesKey = aesKey;
    this.checkpointKey = checkpointKey;
    this.outputMaterial = outputMaterial;
  }

  static async new(key: string): Promise<Encrypt> {
    const argon2 = await import(
      "argon2-browser/dist/argon2-bundled.min.js" as any
    );
    const result = await argon2.hash({
      pass: key,
      salt: SALT,
      type: argon2.ArgonType.Argon2id,
      mem: 19 * 1024, // Memory cost in KiB
      time: 2, // Number of iterations
      parallelism: 1,
      hashLen: 16, // Hash length in bytes
    });
    const raw = Uint8Array.from(
      result.hashHex.match(/.{1,2}/g).map((byte: string) => parseInt(byte, 16)),
    );
    const aesKey = await crypto.subtle.importKey(
      "raw",
      raw,
      { name: "AES-CTR" },
      false,
      ["encrypt"],
    );
    const checkpointKey = await deriveCheckpointKey(raw);
    const material = await crypto.subtle.importKey("raw", raw, "HKDF", false, [
      "deriveKey",
    ]);
    return new Encrypt(aesKey, checkpointKey, material);
  }

  async zeros(): Promise<Uint8Array> {
    const zeros = new Uint8Array(16);
    const cipher = await crypto.subtle.encrypt(
      { name: "AES-CTR", counter: zeros, length: 64 },
      this.aesKey,
      zeros,
    );
    return new Uint8Array(cipher);
  }

  /** Standalone authenticated daemon state; never reuse an AES-CTR stream. */
  async openCheckpoint(
    nonce: Uint8Array,
    data: Uint8Array,
  ): Promise<Uint8Array> {
    if (
      nonce.length !== 12 ||
      data.length > (4 << 20) + 65536 ||
      data.length < 16
    )
      throw new Error("Invalid terminal checkpoint envelope");
    return new Uint8Array(
      await crypto.subtle.decrypt(
        { name: "AES-GCM", iv: Uint8Array.from(nonce) },
        this.checkpointKey,
        Uint8Array.from(data),
      ),
    );
  }

  async segment(
    streamNum: bigint,
    offset: bigint,
    data: Uint8Array,
  ): Promise<Uint8Array> {
    return this.segmentWithKey(this.aesKey, streamNum, offset, data);
  }

  /** Per-incarnation keys, with a bounded cache shared by output and paste state. */
  async outputSegment(
    epoch: Uint8Array | undefined,
    streamNum: bigint,
    offset: bigint,
    data: Uint8Array,
  ) {
    if (!epoch?.length) return this.segment(streamNum, offset, data);
    if (epoch.length !== 16 || !this.outputMaterial)
      throw new Error("Invalid terminal output encryption epoch");
    const id = Number(streamNum & 0xffffffffn);
    const identity = Array.from(epoch, (byte) =>
      byte.toString(16).padStart(2, "0"),
    ).join("");
    let cached = this.outputKeys.get(id);
    if (cached?.epoch !== identity) {
      const key = deriveOutputKey(this.outputMaterial, epoch);
      cached = { epoch: identity, key };
      this.outputKeys.delete(id);
      if (this.outputKeys.size >= 128)
        this.outputKeys.delete(this.outputKeys.keys().next().value!);
      this.outputKeys.set(id, cached);
    }
    return this.segmentWithKey(await cached.key, streamNum, offset, data);
  }

  private async segmentWithKey(
    key: CryptoKey,
    streamNum: bigint,
    offset: bigint,
    data: Uint8Array,
  ) {
    if (streamNum === 0n) throw new Error("stream number must be nonzero"); // security check)

    const blockNum = offset >> 4n;
    const iv = new Uint8Array(16);
    new DataView(iv.buffer).setBigUint64(0, streamNum);
    new DataView(iv.buffer).setBigUint64(8, blockNum);

    const padBytes = Number(offset % 16n);
    const paddedData = new Uint8Array(padBytes + data.length);
    paddedData.set(data, padBytes);

    const encryptedData = await crypto.subtle.encrypt(
      {
        name: "AES-CTR",
        counter: iv,
        length: 64,
      },
      key,
      paddedData,
    );
    return new Uint8Array(encryptedData, padBytes, data.length);
  }
}

export async function deriveOutputKey(
  material: CryptoKey,
  epoch: Uint8Array,
): Promise<CryptoKey> {
  if (epoch.length !== 16)
    throw new Error("Invalid terminal output encryption epoch");
  return crypto.subtle.deriveKey(
    {
      name: "HKDF",
      hash: "SHA-256",
      salt: new TextEncoder().encode("sshxx/terminal-output/v1"),
      info: Uint8Array.from(epoch),
    },
    material,
    { name: "AES-CTR", length: 128 },
    false,
    ["encrypt"],
  );
}

/** Separate AES-GCM from CTR's publicly transmitted encrypted zero block. */
export async function deriveCheckpointKey(raw: Uint8Array): Promise<CryptoKey> {
  const material = await crypto.subtle.importKey(
    "raw",
    Uint8Array.from(raw),
    "HKDF",
    false,
    ["deriveKey"],
  );
  return crypto.subtle.deriveKey(
    {
      name: "HKDF",
      hash: "SHA-256",
      salt: new TextEncoder().encode("sshxx/terminal-checkpoint/v1"),
      info: new TextEncoder().encode("aes-128-gcm"),
    },
    material,
    { name: "AES-GCM", length: 128 },
    false,
    ["decrypt"],
  );
}
