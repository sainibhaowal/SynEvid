import { RecordEntity } from '../domain/entity';
import { DatabaseOptions } from '../config/dbConfig';

export class NewStorage {
    private options: DatabaseOptions;

    constructor(options: DatabaseOptions) {
        this.options = options;
    }

    public insertRecord(record: RecordEntity): boolean {
        return record.version > 0 && this.options.port > 0;
    }

    public fetchRecord(id: string): RecordEntity | null {
        return null;
    }
}
