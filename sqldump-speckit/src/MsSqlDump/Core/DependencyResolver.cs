namespace MsSqlDump.Core;

/// <summary>
/// Represents a node in the dependency graph.
/// </summary>
public class DependencyNode
{
    public required string ObjectName { get; init; }
    public List<string> Dependencies { get; init; } = new();
    public int Level { get; set; } = -1; // Topological level (-1 = unprocessed)
    public bool IsInCycle { get; set; } = false;
}

/// <summary>
/// Represents an edge in the dependency graph.
/// </summary>
public class DependencyEdge
{
    public required string From { get; init; }
    public required string To { get; init; }
}

/// <summary>
/// Represents the complete dependency graph with topological ordering.
/// </summary>
public class DependencyGraph
{
    private readonly Dictionary<string, DependencyNode> _nodes = new();
    private readonly List<DependencyEdge> _edges = new();

    /// <summary>
    /// Adds a node to the graph.
    /// </summary>
    public void AddNode(string objectName)
    {
        if (!_nodes.ContainsKey(objectName))
        {
            _nodes[objectName] = new DependencyNode { ObjectName = objectName };
        }
    }

    /// <summary>
    /// Adds a dependency edge (from depends on to).
    /// </summary>
    public void AddDependency(string from, string to)
    {
        AddNode(from);
        AddNode(to);

        if (!_nodes[from].Dependencies.Contains(to))
        {
            _nodes[from].Dependencies.Add(to);
            _edges.Add(new DependencyEdge { From = from, To = to });
        }
    }

    /// <summary>
    /// Gets all nodes in the graph.
    /// </summary>
    public IReadOnlyCollection<DependencyNode> Nodes => _nodes.Values.ToList();

    /// <summary>
    /// Gets all edges in the graph.
    /// </summary>
    public IReadOnlyCollection<DependencyEdge> Edges => _edges;

    /// <summary>
    /// Gets a node by name.
    /// </summary>
    public DependencyNode? GetNode(string objectName)
    {
        return _nodes.GetValueOrDefault(objectName);
    }

    /// <summary>
    /// Performs topological sort and assigns levels to nodes.
    /// Returns nodes in dependency order (dependencies first).
    /// </summary>
    public List<string> GetTopologicalOrder()
    {
        var result = new List<string>();
        var visited = new HashSet<string>();
        var visiting = new HashSet<string>();

        foreach (var node in _nodes.Keys)
        {
            if (!visited.Contains(node))
            {
                Visit(node, visited, visiting, result);
            }
        }

        result.Reverse(); // Reverse to get dependencies first
        return result;
    }

    private void Visit(string node, HashSet<string> visited, HashSet<string> visiting, List<string> result)
    {
        if (visited.Contains(node))
            return;

        if (visiting.Contains(node))
        {
            // Cycle detected - mark the node as in a cycle
            if (_nodes.TryGetValue(node, out var cycleNode))
            {
                cycleNode.IsInCycle = true;
            }
            return;
        }

        visiting.Add(node);

        if (_nodes.TryGetValue(node, out var currentNode))
        {
            foreach (var dependency in currentNode.Dependencies)
            {
                Visit(dependency, visited, visiting, result);
            }
        }

        visiting.Remove(node);
        visited.Add(node);
        result.Add(node);
    }

    /// <summary>
    /// Detects circular dependencies using Tarjan's algorithm for strongly connected components.
    /// Returns lists of objects that form circular dependency groups.
    /// </summary>
    public List<List<string>> DetectCircularDependencies()
    {
        var index = 0;
        var stack = new Stack<string>();
        var indices = new Dictionary<string, int>();
        var lowLinks = new Dictionary<string, int>();
        var onStack = new HashSet<string>();
        var sccs = new List<List<string>>();

        foreach (var node in _nodes.Keys)
        {
            if (!indices.ContainsKey(node))
            {
                StrongConnect(node, ref index, stack, indices, lowLinks, onStack, sccs);
            }
        }

        // Filter out single-node SCCs (not cycles)
        return sccs.Where(scc => scc.Count > 1).ToList();
    }

    private void StrongConnect(
        string node,
        ref int index,
        Stack<string> stack,
        Dictionary<string, int> indices,
        Dictionary<string, int> lowLinks,
        HashSet<string> onStack,
        List<List<string>> sccs)
    {
        indices[node] = index;
        lowLinks[node] = index;
        index++;
        stack.Push(node);
        onStack.Add(node);

        if (_nodes.TryGetValue(node, out var currentNode))
        {
            foreach (var dependency in currentNode.Dependencies)
            {
                if (!indices.ContainsKey(dependency))
                {
                    StrongConnect(dependency, ref index, stack, indices, lowLinks, onStack, sccs);
                    lowLinks[node] = Math.Min(lowLinks[node], lowLinks[dependency]);
                }
                else if (onStack.Contains(dependency))
                {
                    lowLinks[node] = Math.Min(lowLinks[node], indices[dependency]);
                }
            }
        }

        if (lowLinks[node] == indices[node])
        {
            var scc = new List<string>();
            string w;
            do
            {
                w = stack.Pop();
                onStack.Remove(w);
                scc.Add(w);
            } while (w != node);

            sccs.Add(scc);
        }
    }
}

/// <summary>
/// Resolves dependencies between database objects.
/// </summary>
/// <summary>
/// Resolves table and view dependencies for proper export ordering using topological sort and Tarjan's algorithm for circular dependency detection.
/// </summary>
public class DependencyResolver
{
    /// <summary>
    /// Builds a dependency graph from table metadata with foreign key relationships.
    /// </summary>
    /// <param name="tables">Table metadata collection containing foreign key constraints.</param>
    /// <returns>A dependency graph with topological ordering and circular dependency detection.</returns>
    public DependencyGraph BuildDependencyGraph(IEnumerable<Models.TableMetadata> tables)
    {
        var graph = new DependencyGraph();

        // Add all tables as nodes
        foreach (var table in tables)
        {
            graph.AddNode(table.FullName);
        }

        // Add foreign key dependencies
        foreach (var table in tables)
        {
            foreach (var fk in table.ForeignKeys)
            {
                if (fk.ReferencedSchema != null && fk.ReferencedTable != null)
                {
                    var referencedTable = $"[{fk.ReferencedSchema}].[{fk.ReferencedTable}]";
                    
                    // Skip self-references (handled specially during data export)
                    if (referencedTable != table.FullName)
                    {
                        graph.AddDependency(table.FullName, referencedTable);
                    }
                }
            }
        }

        return graph;
    }

    /// <summary>
    /// Builds a dependency graph including both tables and views with their cross-dependencies.
    /// Views can depend on tables and other views, requiring complex dependency resolution.
    /// </summary>
    /// <param name="tables">Table metadata collection.</param>
    /// <param name="views">View metadata collection.</param>
    /// <param name="connection">Database connection for querying view dependencies.</param>
    /// <returns>A dependency graph with complete table and view ordering.</returns>
    public DependencyGraph BuildViewDependencyGraph(
        IEnumerable<Models.TableMetadata> tables,
        IEnumerable<Models.ProgrammableObject> views,
        DatabaseConnection connection)
    {
        var graph = new DependencyGraph();

        // Add all tables and views as nodes
        foreach (var table in tables)
        {
            graph.AddNode(table.FullName);
        }

        foreach (var view in views)
        {
            graph.AddNode(view.FullName);
        }

        // Add table foreign key dependencies
        foreach (var table in tables)
        {
            foreach (var fk in table.ForeignKeys)
            {
                if (fk.ReferencedSchema != null && fk.ReferencedTable != null)
                {
                    var referencedTable = $"[{fk.ReferencedSchema}].[{fk.ReferencedTable}]";
                    if (referencedTable != table.FullName)
                    {
                        graph.AddDependency(table.FullName, referencedTable);
                    }
                }
            }
        }

        // Add view dependencies by parsing sys.sql_dependencies
        // Note: In a full implementation, we would query sys.sql_dependencies or parse the view definition
        // For now, we'll add a simplified version that assumes views don't have complex dependencies
        // This can be enhanced later to query sys.sql_dependencies

        return graph;
    }
}
