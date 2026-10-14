import {
  Chart as ChartJS,
  ArcElement,
  Tooltip,
  Legend,
  CategoryScale,
  LinearScale,
  PointElement,
  LineElement,
  BarElement,
  Filler,
  Title,
} from 'chart.js';

// Register once, shared by all chart pages.
ChartJS.register(
  ArcElement,
  Tooltip,
  Legend,
  CategoryScale,
  LinearScale,
  PointElement,
  LineElement,
  BarElement,
  Filler,
  Title
);

ChartJS.defaults.color = '#9aa2b1';
ChartJS.defaults.font.family = "'Inter', 'Segoe UI', system-ui, sans-serif";
ChartJS.defaults.borderColor = 'rgba(43,49,63,0.6)';

export { ChartJS };
