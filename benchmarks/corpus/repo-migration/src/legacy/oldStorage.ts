import { RecordEntity } from '../domain/entity';

export class OldStorage {
    private store: Map<string, RecordEntity> = new Map();

    public saveOld(entity: RecordEntity): void {
        this.store.set(entity.recordId, entity);
    }

    public getOld(id: string): RecordEntity | undefined {
        return this.store.get(id);
    }

    public listOld(): RecordEntity[] {
        return Array.from(this.store.values());
    }
}
