export const STARLANE_VERSION =
  typeof __STARLANE_VERSION__ === "string" && __STARLANE_VERSION__.trim()
    ? __STARLANE_VERSION__.trim()
    : "1.5.6";

export const STARLANE_GITHUB_REPOSITORY = "https://github.com/hbftechnologies/starlanenotes";
export const STARLANE_GITEA_REPOSITORY =
  "https://github.com/hbftechnologies/starlanenotes";

export function getStarlaneRepositoryUrl(
  locationLike?: Pick<Location, "hostname">,
): string {
  const hostname =
    locationLike?.hostname ?? (typeof window !== "undefined" ? window.location.hostname : "");
  return hostname.toLowerCase() === "github.com"
    ? STARLANE_GITEA_REPOSITORY
    : STARLANE_GITHUB_REPOSITORY;
}
