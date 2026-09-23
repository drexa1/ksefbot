import {Env} from "../worker";
import {getOpenApiSpec, swaggerHtml} from "../api/openapi";
import {scalarHtml} from "../api/scalar";
import {corsHeaders} from "../auth";
import {AuthError} from "../types/auth";
import {verificationHandler, messageHandler, testSendout } from "./whatsapp";

export type Routes = Partial<Record<Method, Route>>;
export type Method = "GET" | "POST" | "PUT" | "DELETE" | "OPTIONS";
export type Route = (req: Request, env: Env) => Promise<Response>;

const withErrorHandling = (routes: Routes): Routes => {
    const routesWithAuth: Routes = {};
    for (const [method, route] of Object.entries(routes)) {
        routesWithAuth[method as Method] = async (req: Request, env: Env) => {
            try {
                return await route(req, env);
            } catch (error: unknown) {
                console.error(error);
                if (error instanceof AuthError)
                    return Response.json({ error: error.message, details: error.details }, { status: error.status, headers: corsHeaders });
                return Response.json({ error: error }, { status: 500, headers: corsHeaders });
            }
        };
    }
    return routesWithAuth;
};

export const routes: Record<string, Routes> =  {
    //🔓 Not requiring authentication
    "/":                            { GET: async () => new Response(swaggerHtml, { headers: { "Content-Type": "text/html" }}) },
    "/swagger":                     { GET: async () => new Response(swaggerHtml, { headers: { "Content-Type": "text/html" }}) },
    "/docs":                        { GET: async () => new Response(scalarHtml,  { headers: { "Content-Type": "text/html" }}) },
    "/openapi.json":                { GET: async () => Response.json(getOpenApiSpec()) },
    "/whatsapp/webhooks":           withErrorHandling({ GET: verificationHandler, POST: messageHandler }),
    "/whatsapp/flows/onboarding/1": withErrorHandling({  }),
    //🔒 Requiring authentication
    "/whatsapp/test":               withErrorHandling({ POST: testSendout })
};