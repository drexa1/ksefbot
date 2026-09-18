export const getOpenApiSpec = () => ({
    openapi: "3.0.0",
    info: {
        title: "WhatsApp API",
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
        { name: "Test" }
    ],
    paths: {
        "/whatsapp/test": {
            post: {
                summary: "Send a test WhatsApp message",
                tags: ["WhatsApp"],
                security: [{ ApiKeyAuth: [] }],
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
                                        description: "Recipient WhatsApp phone number.",
                                        example: "48518121343"
                                    },
                                    message: {
                                        type: "string",
                                        description: "Message to send.",
                                        example: "Hello!"
                                    }
                                }
                            }
                        }
                    }
                },
                responses: {
                    "200": { description: "WhatsApp message sent successfully." },
                    "400": { description: "Both 'to' and 'message' are required." },
                    "401": { description: "Unauthorized." }
                }
            }
        }
    }
});

export const swaggerHtml = `
	<!DOCTYPE html>
	<html lang="en">
		<head>
			<title>API docs</title>
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
					docExpansion: "none"
				});
			</script>
		</body>
	</html>
`;
