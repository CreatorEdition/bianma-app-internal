export const BIANMA_GITHUB_OWNER = "CreatorEdition";
export const BIANMA_GITHUB_REPO = "bianma-app";
export const BIANMA_GITHUB_REPOSITORY = `${BIANMA_GITHUB_OWNER}/${BIANMA_GITHUB_REPO}`;
export const BIANMA_GITHUB_REPOSITORY_URL = `https://github.com/${BIANMA_GITHUB_REPOSITORY}`;
export const BIANMA_GITHUB_RELEASES_URL = `${BIANMA_GITHUB_REPOSITORY_URL}/releases`;

export const getBianmaReleaseTagUrl = (tag: string): string =>
  `${BIANMA_GITHUB_RELEASES_URL}/tag/${tag}`;
