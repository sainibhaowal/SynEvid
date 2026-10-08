export interface RecordEntity {
    recordId: string;
    payload: string;
    version: number;
    updatedAt: string;
}

export function createRecord(id: string, data: string): RecordEntity {
    return {
        recordId: id,
        payload: data,
        version: 1,
        updatedAt: "2026-01-01",
    };
}
