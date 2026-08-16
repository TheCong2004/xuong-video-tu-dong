/** @jest-environment jsdom */
import React from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { StepDetailModal } from '../../app/src/pages/FlowordStudio/src/components/StepDetailModal';
import { INITIAL_STEP_CONFIGS, StepRun } from '../../app/src/pages/FlowordStudio/src/services/workflowEngine';

function step(status: StepRun['status']): StepRun {
  return { ...INITIAL_STEP_CONFIGS[0], status, progress: 0, logs: [], artifacts: [], retryCount: 0 };
}

describe('Floword module ERRORS tab', () => {
  test('failed step without structured error never shows the green success message', () => {
    render(<StepDetailModal step={step('failed')} onClose={() => undefined} onRetryStep={() => undefined} />);
    fireEvent.click(screen.getByRole('button', { name: 'errors' }));
    expect(screen.getByText('Step failed but no structured error was recorded.')).toBeInTheDocument();
    expect(screen.queryByText(/Không có lỗi nào/)).not.toBeInTheDocument();
  });

  test('successful step with no error may show no errors recorded', () => {
    render(<StepDetailModal step={step('succeeded')} onClose={() => undefined} onRetryStep={() => undefined} />);
    fireEvent.click(screen.getByRole('button', { name: 'errors' }));
    expect(screen.getByText(/Không có lỗi nào được ghi nhận/)).toBeInTheDocument();
  });
});
