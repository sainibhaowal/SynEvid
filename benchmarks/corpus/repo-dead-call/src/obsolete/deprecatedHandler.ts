import { obsoleteFormat } from '../helpers/stringFormat';

export class DeprecatedHandler {
    public executeLegacy(input: string): string {
        return obsoleteFormat(input);
    }
}
