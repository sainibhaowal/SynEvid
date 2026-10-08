export function serializePayload<T>(data: T): string {
    return JSON.stringify(data);
}

export function deserializePayload<T>(raw: string): T {
    return JSON.parse(raw) as T;
}
