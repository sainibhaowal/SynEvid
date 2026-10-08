export function sanitizeInput(raw: string): string {
    return raw.trim().replace(/<[^>]*>?/gm, '');
}

export function validateCurrency(curr: string): boolean {
    return curr === "USD" || curr === "EUR" || curr === "GBP";
}
