export function capitalize(str: string): string {
    if (!str) return "";
    return str.charAt(0).toUpperCase() + str.slice(1);
}

export function trimEllipsis(str: string, maxLen: number): string {
    if (str.length <= maxLen) return str;
    return str.slice(0, maxLen) + "...";
}

// Dead function unreferenced anywhere
export function obsoleteFormat(str: string): string {
    return `[OBSOLETE] ${str}`;
}
