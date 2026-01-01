import { useEffect, useRef } from 'react';
import * as d3 from 'd3';
import type { Entity, Relationship, EntityType } from '../types';
import * as styles from '../styles/graph.css';
import { vars } from '../styles/theme.css';
import { getEntityColor } from '../utils/colors';

interface Props {
  entities: Entity[];
  relationships: Relationship[];
  onNodeClick?: (entityId: string) => void;
}

interface NodeDatum extends d3.SimulationNodeDatum {
  id: string;
  label: string;
  type: EntityType;
}

interface LinkDatum extends d3.SimulationLinkDatum<NodeDatum> {
  label: string;
}

export function EntityGraph({ entities, relationships, onNodeClick }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    if (!svgRef.current || entities.length === 0) return;

    const svg = d3.select(svgRef.current);
    const width = svgRef.current.clientWidth;
    const height = svgRef.current.clientHeight;

    svg.selectAll('*').remove();

    const nodes: NodeDatum[] = entities.map((e) => ({
      id: e.id,
      label: e.canonical_name,
      type: e.type as EntityType,
    }));

    const nodeIds = new Set(nodes.map((n) => n.id));
    const links: LinkDatum[] = relationships
      .filter((r) => nodeIds.has(r.source_id) && nodeIds.has(r.target_id))
      .map((r) => ({
        source: r.source_id,
        target: r.target_id,
        label: r.relation_type.replace(/_/g, ' '),
      }));

    const simulation = d3
      .forceSimulation(nodes)
      .force('link', d3.forceLink<NodeDatum, LinkDatum>(links).id((d) => d.id).distance(120))
      .force('charge', d3.forceManyBody().strength(-300))
      .force('center', d3.forceCenter(width / 2, height / 2))
      .force('collision', d3.forceCollide().radius(40));

    const g = svg.append('g');

    const zoom = d3.zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.1, 4])
      .on('zoom', (event) => {
        g.attr('transform', event.transform);
      });

    svg.call(zoom);

    const link = g
      .append('g')
      .selectAll('line')
      .data(links)
      .join('line')
      .attr('stroke', '#666')
      .attr('stroke-opacity', 0.6)
      .attr('stroke-width', 1.5);

    const linkLabel = g
      .append('g')
      .selectAll('text')
      .data(links)
      .join('text')
      .attr('font-size', 10)
      .attr('fill', '#888')
      .attr('text-anchor', 'middle')
      .text((d) => d.label);

    const node = g
      .append('g')
      .selectAll('circle')
      .data(nodes)
      .join('circle')
      .attr('r', 12)
      .attr('fill', (d) => getEntityColor(d.type))
      .attr('stroke', '#fff')
      .attr('stroke-width', 2)
      .style('cursor', 'pointer')
      .on('click', (_, d) => {
        if (onNodeClick) onNodeClick(d.id);
      })
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      .call(
        d3.drag<any, NodeDatum>()
          .on('start', (event, d) => {
            if (!event.active) simulation.alphaTarget(0.3).restart();
            d.fx = d.x;
            d.fy = d.y;
          })
          .on('drag', (event, d) => {
            d.fx = event.x;
            d.fy = event.y;
          })
          .on('end', (event, d) => {
            if (!event.active) simulation.alphaTarget(0);
            d.fx = null;
            d.fy = null;
          })
      );

    const nodeLabel = g
      .append('g')
      .selectAll('text')
      .data(nodes)
      .join('text')
      .attr('font-size', 11)
      .attr('fill', vars.color.textPrimary)
      .attr('text-anchor', 'middle')
      .attr('dy', 25)
      .text((d) => d.label);

    simulation.on('tick', () => {
      link
        .attr('x1', (d) => (d.source as NodeDatum).x!)
        .attr('y1', (d) => (d.source as NodeDatum).y!)
        .attr('x2', (d) => (d.target as NodeDatum).x!)
        .attr('y2', (d) => (d.target as NodeDatum).y!);

      linkLabel
        .attr('x', (d) => ((d.source as NodeDatum).x! + (d.target as NodeDatum).x!) / 2)
        .attr('y', (d) => ((d.source as NodeDatum).y! + (d.target as NodeDatum).y!) / 2);

      node.attr('cx', (d) => d.x!).attr('cy', (d) => d.y!);

      nodeLabel.attr('x', (d) => d.x!).attr('y', (d) => d.y!);
    });

    return () => {
      simulation.stop();
    };
  }, [entities, relationships, onNodeClick]);

  return <svg ref={svgRef} className={styles.graphContainer} />;
}
