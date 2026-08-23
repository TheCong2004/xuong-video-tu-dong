globalThis.__flowordProduction = {
  health: (profileId) => ({ ok: true, profileId, capabilities: ['grok.image.edit'] }),
  dispatch: (request) => ({ ok: true, requestId: request.requestId, jobId: request.jobId })
};
