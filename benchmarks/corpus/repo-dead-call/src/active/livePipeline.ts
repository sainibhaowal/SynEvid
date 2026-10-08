import { capitalize, trimEllipsis } from '../helpers/stringFormat';

export class LiveDataPipeline {
    public processMessage(raw: string): string {
        const cleaned = raw.trim();
        const capped = capitalize(cleaned);
        return trimEllipsis(capped, 50);
    }
}
