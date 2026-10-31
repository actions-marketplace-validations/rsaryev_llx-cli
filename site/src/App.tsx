import { Footer } from './components/Footer';
import { Hero } from './components/Hero';
import { Loop } from './components/Loop';
import { Nav } from './components/Nav';

export function App() {
  return (
    <>
      <Nav />
      <main id="main">
        <Hero />
        <Loop />
      </main>
      <Footer />
    </>
  );
}
