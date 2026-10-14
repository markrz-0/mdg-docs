import { describe, expect, it } from 'vitest';
import type { Selection, View } from '../src/api';
import { layoutAll, layoutChildren } from '../src/graph/autoLayout';
import { BOUNDARY_IN, BOUNDARY_OUT, buildGraph } from '../src/graph/buildGraph';
import { boundaryKey } from '../src/state/layoutKeys';
import fixture from './fixtures/authentication.view.json';

// The session's view of examples/authentication.mdg (kept in sync by a Rust test).
const view = fixture as unknown as View;
const doc = view.document;

describe('buildGraph', () => {
  it('shows only direct children at the root, without boundary nodes', () => {
    const { nodes, edges } = buildGraph(doc, 'application', view.positions, null);
    expect(nodes.map((n) => n.id)).toEqual(['frontend', 'api-service', 'jwt-validator']);
    expect(edges).toHaveLength(6);
  });

  it('renders the current module as boundary nodes, not as a child', () => {
    const { nodes, edges } = buildGraph(doc, 'user-routes', view.positions, null);
    expect(nodes.map((n) => n.id)).toEqual([BOUNDARY_IN, 'protected-endpoint', BOUNDARY_OUT]);
    const request = edges.find((e) => e.id === 'routes-request')!;
    expect(request).toMatchObject({ source: BOUNDARY_IN, sourceHandle: 'profile-request', target: 'protected-endpoint' });
    const out = edges.find((e) => e.id === 'routes-validate-token')!;
    expect(out).toMatchObject({ source: 'protected-endpoint', target: BOUNDARY_OUT, targetHandle: 'token-validation-request' });
  });

  it('uses stored positions, including moved boundary panels', () => {
    const { nodes } = buildGraph(doc, 'application', view.positions, null);
    expect(nodes.find((n) => n.id === 'api-service')!.position).toEqual({ x: 380, y: 40 });
    const positions = { ...view.positions, [boundaryKey('api-service', 'input')]: { x: -500, y: 40 } };
    const inside = buildGraph(doc, 'api-service', positions, null);
    const panel = inside.nodes.find((n) => n.id === BOUNDARY_IN)!;
    expect(panel.position).toEqual({ x: -500, y: 40 });
    expect(panel.draggable).toBe(true);
  });

  it('marks the selected module and connection', () => {
    const module: Selection = { kind: 'module', moduleId: 'frontend' };
    expect(buildGraph(doc, 'application', view.positions, module).nodes.find((n) => n.selected)?.id).toBe('frontend');
    const connection: Selection = { kind: 'connection', connectionId: 'root-login' };
    expect(buildGraph(doc, 'application', view.positions, connection).edges.find((e) => e.selected)?.id).toBe('root-login');
  });
});

describe('port highlighting', () => {
  it('highlights every connection from or to the selected port', () => {
    const port: Selection = { kind: 'port', moduleId: 'api-service', portId: 'login-response' };
    const root = buildGraph(doc, 'application', view.positions, port);
    expect(root.edges.filter((e) => e.className === 'highlighted').map((e) => e.id)).toEqual(['root-login-response']);
    // Inside API Service the same port is a boundary output fed by two sources.
    const inside = buildGraph(doc, 'api-service', view.positions, port);
    expect(inside.edges.filter((e) => e.className === 'highlighted').map((e) => e.id)).toEqual([
      'api-token-issued',
      'api-login-failed',
    ]);
    const none = buildGraph(doc, 'application', view.positions, null);
    expect(none.edges.some((e) => e.className === 'highlighted')).toBe(false);
  });
});

describe('auto-layout', () => {
  const IN = (id: string) => boundaryKey(id, 'input');
  const OUT = (id: string) => boundaryKey(id, 'output');

  it('lays out a level left to right, boundary inputs first and outputs last', () => {
    const positions = layoutChildren(doc, 'authentication');
    expect(Object.keys(positions).sort()).toEqual(
      [IN('authentication'), OUT('authentication'), 'credential-validator', 'token-issuer'].sort(),
    );
    const x = (key: string) => positions[key]!.x;
    expect(x('token-issuer')).toBeGreaterThan(x('credential-validator'));
    expect(x(IN('authentication'))).toBe(0);
    expect(x(IN('authentication'))).toBeLessThan(x('credential-validator'));
    expect(x(OUT('authentication'))).toBeGreaterThan(x('token-issuer'));
  });

  it('places no boundary panels at the root and separates nodes', () => {
    const positions = layoutChildren(doc, 'application');
    expect(Object.keys(positions).sort()).toEqual(['api-service', 'frontend', 'jwt-validator']);
    const xs = Object.values(positions).map((p) => p.x).sort((a, b) => a - b);
    for (let i = 1; i < xs.length; i += 1) expect(xs[i]! - xs[i - 1]!).toBeGreaterThanOrEqual(200);
  });

  it('lays out every level, including the boundary panels of each parent', () => {
    const positions = layoutAll(doc);
    const parents = ['api-service', 'authentication', 'user-routes'];
    expect(Object.keys(positions).sort()).toEqual(
      [...Object.keys(doc.modules).filter((id) => id !== doc.rootId), ...parents.flatMap((id) => [IN(id), OUT(id)])].sort(),
    );
  });

  it('handles leaf modules and single-child levels', () => {
    expect(layoutChildren(doc, 'frontend')).toEqual({});
    const positions = layoutChildren(doc, 'user-routes');
    expect(positions[IN('user-routes')]!.x).toBe(0);
    expect(positions['protected-endpoint']!.x).toBeGreaterThan(0);
    expect(positions[OUT('user-routes')]!.x).toBeGreaterThan(positions['protected-endpoint']!.x);
  });
});
