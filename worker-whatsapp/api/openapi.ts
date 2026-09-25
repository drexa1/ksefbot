export const getOpenApiSpec = () => ({
    openapi: "3.0.0",
    info: {
        title: "ksefbot-whatsapp-api",
        version: "1.0.0"
    },
    components: {
        securitySchemes: {
            ApiKeyAuth: {
                type: "apiKey",
                in: "header",
                name: "X-API-Key",
                description: "Clients can authenticate using this header."
            }
        }
    },
    tags: [
        {name: "Whatsapp"}
    ],
    paths: {
        "/whatsapp/test-message": {
            post: {
                summary: "Send a test message",
                tags: ["Whatsapp"],
                security: [{ApiKeyAuth: []}],
                requestBody: {
                    required: true,
                    content: {
                        "application/json": {
                            schema: {
                                type: "object",
                                additionalProperties: false,
                                required: ["to", "message"],
                                properties: {
                                    to: {
                                        type: "string",
                                        description: "Recipient phone number.",
                                        example: "+48518121343"
                                    },
                                    message: {
                                        type: "string",
                                        description: "Message to send.",
                                        example: "Hello 👋"
                                    }
                                }
                            }
                        }
                    }
                },
                responses: {
                    "200": {description: "Whatsapp message sent successfully."},
                    "400": {description: "Missing or invalid message parameters."},
                    "401": {description: "Unauthorized."}
                }
            }
        },

        "/whatsapp/test-flow": {
            post: {
                summary: "Send a test WhatsApp Flow",
                tags: ["Whatsapp"],
                security: [{ApiKeyAuth: []}],
                requestBody: {
                    required: true,
                    content: {
                        "application/json": {
                            schema: {
                                type: "object",
                                additionalProperties: false,
                                required: ["to", "flowId"],
                                properties: {
                                    to: {
                                        type: "string",
                                        description: "Recipient phone number.",
                                        example: "+48518121343"
                                    },
                                    flowId: {
                                        type: "string",
                                        description: "Whatsapp Flow ID.",
                                        example: "1084945671125075"
                                    }
                                }
                            }
                        }
                    }
                },
                responses: {
                    "200": {description: "Whatsapp Flow sent successfully."},
                    "400": {description: "Missing or invalid Flow parameters."},
                    "401": {description: "Unauthorized."}
                }
            }
        }
    }
});

export const swaggerHtml = `
    <!DOCTYPE html>
    <html lang="en">
        <head>
            <title>ksefbot-whatsapp-api</title>
            <link rel="icon" type="image/png" href="https://static.vecteezy.com/system/resources/previews/016/716/480/non_2x/whatsapp-icon-free-png.png"/>
            <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist/swagger-ui.css"/>
        </head>
        <body>
            <div id="swagger-ui"></div>
            <script src="https://unpkg.com/swagger-ui-dist/swagger-ui-bundle.js"></script>
            <script>
                // noinspection JSUnresolvedVariable
                SwaggerUIBundle({
                    url: "/openapi.json",
                    dom_id: "#swagger-ui",
                    // docExpansion: "none"
                });
            </script>
        </body>
    </html>
`;
