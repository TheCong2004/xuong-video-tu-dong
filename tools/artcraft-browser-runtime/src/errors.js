'use strict';

const SAFE_CODE = /^[A-Z0-9_:-]+$/;

class RuntimeError extends Error {
  constructor(code, message, status = 500, details = {}, retryable = false) {
    super(message);
    this.name = 'RuntimeError';
    this.code = code;
    this.status = status;
    this.details = details;
    this.retryable = retryable;
  }
}

function asRuntimeError(error) {
  if (error instanceof RuntimeError) return error;
  return new RuntimeError('INTERNAL_RUNTIME_ERROR', 'Local browser runtime failure', 500, {}, false);
}

function envelope(error) {
  const value = asRuntimeError(error);
  const code = SAFE_CODE.test(value.code) ? value.code : 'INTERNAL_RUNTIME_ERROR';
  return {
    error: {
      code,
      message: value.message || 'Local browser runtime failure',
      retryable: Boolean(value.retryable),
      details: value.details && typeof value.details === 'object' ? value.details : {},
    },
  };
}

module.exports = { RuntimeError, asRuntimeError, envelope };
