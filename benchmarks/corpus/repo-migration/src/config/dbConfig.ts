export interface DatabaseOptions {
    host: string;
    port: number;
    maxConnections: number;
}

export const defaultDbConfig: DatabaseOptions = {
    host: "localhost",
    port: 5432,
    maxConnections: 10,
};
