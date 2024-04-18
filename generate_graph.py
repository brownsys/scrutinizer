from collections import defaultdict

import json
import networkx as nx
import os
import sys

VISUALIZE_PASSING = False
VISUALIZE_FAILING = True


def draw_passing(graph, data):
    for node in data['passing']:
        function = node['function']
        from_node = function['def_id']

        if node['allowlisted']:
            graph.add_node(from_node, label=from_node + " -A",
                           fillcolor="orchid", style="filled")
        elif len(node['important_locals']) == 0:
            graph.add_node(from_node, label=from_node + " -I",
                           fillcolor="khaki", style="filled")
        else:
            graph.add_node(from_node, label=from_node,
                           fillcolor="lightgreen", style="filled")
            for call in function.get('calls', []):
                if not graph.has_edge(call['def_id'], from_node):
                    graph.add_edge(from_node, call['def_id'])


def draw_failing(graph, data):
    for node in data['failing']:
        function = node['function']
        from_node = function['def_id']
        label = from_node
        fillcolor = "tomato"

        if node['raw_pointer_deref']:
            label += " -R"
            fillcolor = "orange"

        if node['has_transmute']:
            label += " -T"
            fillcolor = "orange"

        if not function['has_body']:
            label += " -B"
            fillcolor = "orange"

        graph.add_node(from_node, label=label,
                       fillcolor=fillcolor, style="filled")

        for call in function.get('calls', []):
            if not graph.has_edge(call['def_id'], from_node):
                graph.add_edge(from_node, call['def_id'])


with open(sys.argv[1]) as file:
    input = json.loads(file.read())
    for graph in input['results']:
        G = nx.DiGraph()

        if VISUALIZE_PASSING:
            draw_passing(G, graph)

        if VISUALIZE_FAILING:
            draw_failing(G, graph)

        B = nx.dag_to_branching(G)
        sources = defaultdict(set)
        for v, source in B.nodes(data="source"):
            sources[source].add(v)

        for source, nodes in sources.items():
            for v in nodes:
                B.nodes[v].update(G.nodes[source])

        A = nx.nx_agraph.to_agraph(B)

        if not os.path.exists("callgraphs"):
            os.mkdir("callgraphs")

        A.draw(f"callgraphs/{graph['def_id']
                             }.callgraph.svg", format="svg", prog="dot")
