import { api } from "@/lib/api";
import { dataClient } from "@/lib/dataClient";

const unavailable = async (): Promise<never> => {
  throw new Error(
    "Connected services and asset storage are unavailable in this fixture prototype.",
  );
};

/** Called only by the standalone review entry, never by the application router. */
export function installTemplatePrototypeGuards() {
  // The shared editor exposes uploads, bookmarks and other connected commands.
  // Keep those commands from touching real APIs or the user's local vault.
  for (const [name, member] of Object.entries(dataClient)) {
    if (typeof member === "function") Reflect.set(dataClient, name, unavailable);
  }
  api.fetch = unavailable;
}
