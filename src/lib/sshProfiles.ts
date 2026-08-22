import type { WsSshProfile } from "$lib/protocol";

export function uniqueSshProfileName(profiles: WsSshProfile[]) {
  const used = new Set(
    profiles.map((profile) => profile.name.toLocaleLowerCase()),
  );
  for (let suffix = 1; ; suffix += 1) {
    const name = suffix === 1 ? "SSH connection" : `SSH connection ${suffix}`;
    if (!used.has(name.toLocaleLowerCase())) return name;
  }
}

export function createSshProfileDraft(
  profiles: WsSshProfile[],
  theme: string,
): WsSshProfile {
  const id = crypto.randomUUID
    ? crypto.randomUUID()
    : Array.from(crypto.getRandomValues(new Uint8Array(16)), (byte) =>
        byte.toString(16).padStart(2, "0"),
      ).join("");
  return {
    id,
    name: uniqueSshProfileName(profiles),
    host: "",
    port: 22,
    username: "",
    authMethod: "default",
    keyPath: "",
    acceptNewHostKey: true,
    theme,
    backgroundEnabled: false,
    background: "#181818",
  };
}
