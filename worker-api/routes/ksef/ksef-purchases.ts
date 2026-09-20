import {Env} from "../../worker";
import {getInvoices} from "./ksef";

/**
 * Invoices where the user is the invoicee.
 */
export async function get(req: Request, env: Env): Promise<Response> {
    return await getInvoices(req, env, "Subject2");
}