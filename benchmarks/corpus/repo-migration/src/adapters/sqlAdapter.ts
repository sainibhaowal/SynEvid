import { NewStorage } from '../modern/newStorage';
import { RecordEntity } from '../domain/entity';
import { defaultDbConfig } from '../config/dbConfig';

export class SqlAdapter {
    private storage: NewStorage;

    constructor() {
        this.storage = new NewStorage(defaultDbConfig);
    }

    public executeInsert(entity: RecordEntity): boolean {
        return this.storage.insertRecord(entity);
    }
}
