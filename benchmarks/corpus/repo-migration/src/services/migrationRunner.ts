import { OldStorage } from '../legacy/oldStorage';
import { SqlAdapter } from '../adapters/sqlAdapter';

export class MigrationRunner {
    constructor(private oldStorage: OldStorage, private adapter: SqlAdapter) {}

    public migrateAll(): number {
        const records = this.oldStorage.listOld();
        let count = 0;
        for (const r of records) {
            if (this.adapter.executeInsert(r)) {
                count++;
            }
        }
        return count;
    }
}
